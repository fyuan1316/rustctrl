use crate::{Error, Result};
use futures::StreamExt;
use k8s_openapi::chrono::{DateTime, Utc};
use kube::{
    api::{Api, ListParams, Patch, PatchParams},
    client::Client,
    runtime::{
        controller::Action,
        events::{Event, EventType, Recorder, Reporter},
        finalizer::{finalizer, Event as EventFinalizer},
        Controller,
    },
    CustomResource, Resource, ResourceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::{sync::RwLock, time::Duration};
use tracing::*;
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "MyWorkLoad",
    group = "mars.org",
    version = "v1alpha1",
    namespaced
)]
#[kube(status = "MyWorkLoadStatus", shortname = "mywl")]
pub struct MyWorkLoadSpec {
    pub image: String,
    pub replicas: u8,
}
#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct MyWorkLoadStatus {
    pub hidden: bool,
}

impl MyWorkLoad {
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        let recorder = ctx.diagnostics.read().await.recorder(client.clone(), self);
        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let instance: Api<MyWorkLoad> = Api::namespaced(client, &ns);

        if name == "illegal" {
            return Err(Error::IllegalDocument);
        }
        let should_hide = false;
        let new_status = Patch::Apply(json!({
            "apiVersion": "mars.org/v1alpha1",
            "kind": "MyWorkLoad",
            "status": MyWorkLoadStatus{
                hidden: should_hide,
            }
        }));
        let ps = PatchParams::apply("rustctrl").force();
        let _o = instance
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;
        Ok(Action::requeue(Duration::from_secs(1 * 60)))
    }

    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let recorder = ctx
            .diagnostics
            .read()
            .await
            .recorder(ctx.client.clone(), self);
        recorder
            .publish(Event {
                type_: EventType::Normal,
                reason: "DeleteRequested".into(),
                note: Some(format!("Delete `{}`", self.name_any())),
                action: "Deleting".into(),
                secondary: None,
            })
            .await
            .map_err(Error::KubeError)?;
        Ok(Action::await_change())
    }
}

#[derive(Clone)]
pub struct Context {
    pub client: Client,
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    // pub metrics:Metrics,
}
pub static FINALIZER: &str = "documents.kube.rs";
async fn reconciler(mywl: Arc<MyWorkLoad>, ctx: Arc<Context>) -> Result<Action> {
    let ns = mywl.namespace().unwrap();
    let wls: Api<MyWorkLoad> = Api::namespaced(ctx.client.clone(), &ns);
    finalizer(&wls, FINALIZER, mywl, |event| async {
        match event {
            EventFinalizer::Apply(mywl) => mywl.reconcile(ctx.clone()).await,
            EventFinalizer::Cleanup(mywl) => mywl.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
    // instance.reconcile(ctx.clone()).await
}

#[derive(Clone, Default)]
pub struct State {
    diagnostics: Arc<RwLock<Diagnostics>>,
    registry: prometheus::Registry,
}

impl State {
    pub fn to_context(&self, client: Client) -> Arc<Context> {
        Arc::new(Context {
            client,
            diagnostics: self.diagnostics.clone(),
        })
    }
}

pub async fn run(state: State) {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let mywls = Api::<MyWorkLoad>::all(client.clone());
    if let Err(e) = mywls.list(&ListParams::default().limit(1)).await {
        error!("CRD is not queryable;{e:?}. Is the CRD installed?");
        info!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        std::process::exit(1);
    }
    Controller::new(mywls, ListParams::default())
        .shutdown_on_signal()
        .run(reconciler, error_policy, state.to_context(client))
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}

fn error_policy(mywl: Arc<MyWorkLoad>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    Action::requeue(Duration::from_secs(10))
}

#[derive(Clone, Serialize)]
pub struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "myworkload-controller".into(),
        }
    }
}
impl Diagnostics {
    fn recorder(&self, client: Client, instance: &MyWorkLoad) -> Recorder {
        Recorder::new(client, self.reporter.clone(), instance.object_ref(&()))
    }
}
