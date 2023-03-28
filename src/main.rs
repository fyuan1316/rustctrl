use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    runtime::{controller::Action, Controller},
    Api, Client, ResourceExt,
};
use rustctrl::ctrl::{self, State};

#[derive(thiserror::Error, Debug)]
pub enum Error {}
pub type Result<T, E = Error> = std::result::Result<T, E>;
async fn reconcile(obj: Arc<Pod>, ctx: Arc<()>) -> Result<Action> {
    println!("reconcile request: {}", obj.name_any());
    Ok(Action::requeue(Duration::from_secs(20)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("start!");
    let state = State::default();

    // let controller = ctrl::run(state.clone());
    // tokio::spawn(controller);
    // Ok(())
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::<Pod>::all(client);

    Controller::new(pods.clone(), Default::default())
        .run(reconcile, error_policy, Arc::new(()))
        .for_each(|_| futures::future::ready(()))
        .await;

    Ok(())
}
fn error_policy(_object: Arc<Pod>, _err: &Error, _ctx: Arc<()>) -> Action {
    Action::requeue(Duration::from_secs(5))
}
