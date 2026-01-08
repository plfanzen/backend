use std::{sync::Arc, time::Duration};

use k8s_openapi::api::core::v1::Service;
use kube::{
    Api, Client, Error,
    runtime::{Controller, controller::Action, watcher},
};

use crate::{cr::SSHGateway, gateway::BackendRegistry};

use futures_util::StreamExt;

struct Data {
    /// kubernetes client
    client: Client,
    /// Backend registry to manage backends
    backend_registry: BackendRegistry,
}

async fn reconcile(object: Arc<SSHGateway>, ctx: Arc<Data>) -> Result<Action, Error> {
    let backend_registry = &ctx.backend_registry;
    let spec = &object.spec;
    let Some(ref ns) = object.metadata.namespace else {
        // This is always namespaced, so this should be unreachable, but let's just return a requeue
        tracing::error!("Failed to get namespace!");
        return Ok(Action::requeue(Duration::from_secs(60)));
    };
    let api: Api<Service> = Api::namespaced(ctx.client.clone(), ns);
    if api.get_opt(&spec.backend_service).await?.is_none() {
        // Reconcile after 10 seconds for non-existent services
        // TODO: Backoff
        return Ok(Action::requeue(Duration::from_secs(10)));
    }
    let backend_name = format!("{}-{}", spec.backend_service, ns);
    if object.metadata.deletion_timestamp.is_some() {
        backend_registry.remove_backend(&backend_name).await;
        return Ok(Action::await_change());
    }
    backend_registry
        .add_backend(
            backend_name,
            crate::gateway::BackendConfig {
                addr: format!(
                    "{}.{}.svc.cluster.local:{}",
                    spec.backend_service, ns, spec.backend_port
                ),
                user: spec.backend_username.clone(),
                pass: spec.backend_password.clone(),
                login_pass: spec.gateway_password.clone(),
            },
        )
        .await;
    Ok(Action::await_change())
}

fn error_policy(_obj: Arc<SSHGateway>, error: &Error, _ctx: Arc<Data>) -> Action {
    tracing::error!("Failed to reconcile: {:?}", error);
    Action::requeue(Duration::from_secs(60))
}

pub async fn run_controller(
    client: Client,
    backend_registry: BackendRegistry,
) -> Result<(), Error> {
    let context = Arc::new(Data {
        client: client.clone(),
        backend_registry,
    });
    let api: Api<SSHGateway> = Api::all(client);
    Controller::new(api, watcher::Config::default())
        .run(reconcile, error_policy, context)
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::debug!("reconciled {:?}", o),
                Err(e) => tracing::error!("reconcile failed: {:?}", e),
            }
        })
        .await;

    Ok(())
}
