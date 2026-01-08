mod controller;
mod cr;
mod gateway;

use gateway::Gateway;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{Api, CustomResourceExt};
use rand_core::OsRng;
use russh::server::Server;
use std::sync::Arc;

use crate::cr::SSHGateway;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut config = russh::server::Config::default();
    config.inactivity_timeout = Some(std::time::Duration::from_secs(600));
    config.auth_rejection_time = std::time::Duration::from_millis(500);
    config.keys = vec![russh::keys::PrivateKey::random(
        &mut OsRng,
        russh::keys::Algorithm::Ed25519,
    )?];
    let config = Arc::new(config);

    let gateway = Gateway::new();

    let socket = tokio::net::TcpListener::bind("0.0.0.0:2222").await?;
    println!("SSH gateway listening on 0.0.0.0:2222");

    // Cloning is not a problem here because there's an Arc<> in the gateway,
    let registry = gateway.backend_registry();

    let client = kube::Client::try_default().await?;
    
    let cr_api: Api<CustomResourceDefinition> = Api::all(client.clone());
    let cr = SSHGateway::crd();
    let cr_name = cr.metadata.name.as_ref().unwrap();
    match cr_api.get_opt(cr_name).await {
        Ok(Some(_)) => {
            tracing::info!("CRD {} already exists", cr_name);
        }
        Ok(None) => {
            tracing::info!("Creating CRD {}", cr_name);
            cr_api.create(&Default::default(), &cr).await?;
            tracing::info!("Created CRD {}", cr_name);
        }
        Err(e) => {
            return Err(e.into());
        }
    }
    // Spawn controller task that can dynamically manage backends
    let controller = tokio::spawn(async move {
        crate::controller::run_controller(client, registry)
            .await
            .expect("Failed to run controller");
    });

    // Run the gateway (this will take ownership but gateway_clone can still manage backends)
    let mut gateway_server = gateway;
    let (res1, res2) = tokio::join!(gateway_server.run_on_socket(config, &socket), controller);
    res1?;
    res2?;
    Ok(())
}
