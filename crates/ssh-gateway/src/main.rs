mod controller;
mod cr;
mod gateway;

use gateway::Gateway;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{Api, CustomResourceExt};
use rand_core::OsRng;
use russh::{
    keys::ssh_key::LineEnding,
    server::{Server, run_stream},
};
use std::sync::Arc;
use tracing::debug;

use crate::cr::SSHGateway;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    rustls::crypto::aws_lc_rs::default_provider().install_default().expect("Failed to set AWS-LC-RS as default TLS provider");

    let key_file =
        std::env::var("PRIVATE_KEY_FILE").unwrap_or_else(|_| "/data/ssh_host_key".to_string());

    let private_key = if std::path::Path::new(&key_file).exists() {
        tracing::info!("Loading private key from {}", key_file);
        let key_data = std::fs::read_to_string(&key_file)?;
        russh::keys::decode_secret_key(&key_data, None)?
    } else {
        tracing::info!("Generating new private key and saving to {}", key_file);
        let key = russh::keys::PrivateKey::random(&mut OsRng, russh::keys::Algorithm::Ed25519)?;

        if let Some(parent) = std::path::Path::new(&key_file).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let key_string = key.to_openssh(LineEnding::LF)?;
        std::fs::write(&key_file, key_string)?;

        key
    };

    let mut config = russh::server::Config::default();
    config.inactivity_timeout = Some(std::time::Duration::from_secs(600));
    config.auth_rejection_time = std::time::Duration::from_millis(350);
    config.keys = vec![private_key];
    config.methods = From::from(&[russh::MethodKind::Password] as &[russh::MethodKind]);
    let config = Arc::new(config);

    let mut gateway = Gateway::new();

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

    tokio::spawn(async move {
        if let Err(e) = crate::controller::run_controller(client, registry).await {
            panic!("Controller failed: {:?}", e);
        }
    });

    loop {
        let (socket, peer_addr) = socket.accept().await?;
        let config = config.clone();
        let handler = gateway.new_client(Some(peer_addr));

        tokio::spawn(async move {
            let session = match run_stream(config, socket, handler).await {
                Ok(s) => s,
                Err(e) => {
                    debug!("Connection setup failed: {:?}", e);
                    return;
                }
            };

            if let Err(e) = session.await {
                debug!("Connection closed with error: {:?}", e);
            } else {
                debug!("Connection closed");
            }
        });
    }
}
