
use std::sync::Arc;
use tracing::*;
use kube::{
    api::{Api, ListParams},
    client::Client,
    CustomResource, ResourceExt, runtime::{controller::Action, Controller},
};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "MyWorkLoad",
    group = "org.mars",
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

#[derive(Clone)]
pub struct Context {
    pub client: Client,
}
async fn reconciler(mywl: Arc<MyWorkLoad>, ctx: Arc<Context>) -> Result<Action> {
    let ns = mywl.namespace().unwrap();
    let instance = Api::namespaced(ctx.client.clone(), &ns);
    Controller::new(instance, lp)
}

pub struct State{
    diagnostics: Arc<RwLock<Diagnostics>>;
    registry: prometheus::Registry,
}
pub async fn run(state: State) {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let mywls = Api::<MyWorkLoad>::all(client.clone());
    if let Err(e) = mywls.list(&ListParams::default().limit(1)).await{
        error!("CRD is not queryable;{e:?}. Is the CRD installed?");
        info!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        std::process::exit(1);
    }
    Controller::new(mywls, ListParams::default())
    .shutdown_on_signal()
    .run(reconciler, error_policy, context)
    
}
