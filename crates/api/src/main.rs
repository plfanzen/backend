// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{convert::Infallible, error::Error, net::SocketAddr, sync::Arc};

use diesel::Connection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use ed25519_dalek::SigningKey;
use hyper::{Method, Response, StatusCode, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use juniper::{EmptySubscription, RootNode};
use juniper_hyper::{graphiql, graphql, playground};
use tokio::net::TcpListener;

use crate::graphql::{AuthenticatedUser, Context, Mutation, Query, Schema};

mod db;
mod graphql;

mod manager_api {
    tonic::include_proto!("plfanzen_ctf");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Set RUST_LOG to debug
    unsafe {
        std::env::set_var("RUST_LOG", "debug");
    }
    tracing_subscriber::fmt::init();

    for var in &[
        "EMAIL_SMTP_SERVER",
        "EMAIL_SMTP_USERNAME",
        "EMAIL_SMTP_PASSWORD",
        "EMAIL_FROM_ADDRESS",
    ] {
        if std::env::var(var).is_err() {
            tracing::warn!(
                "Environment variable {var} is not set; users will be approved automatically!"
            );
        }
    }

    let root_node: Arc<Schema> = Arc::new(RootNode::new(Query, Mutation, EmptySubscription::new()));

    let addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await?;

    let key_file = std::env::var("SIGNING_KEY_FILE").unwrap_or_else(|_| "key.json".to_string());
    let key_file = std::path::Path::new(&key_file);
    if !key_file.exists() {
        let mut csprng = rand::rngs::OsRng;
        let signing_key: SigningKey = SigningKey::generate(&mut csprng);
        let keypair_json = serde_json::to_string_pretty(&signing_key)?;
        std::fs::write(key_file, keypair_json)?;
        tracing::info!("Generated new signing key and saved to key.json");
    }
    let keypair_json = std::fs::read_to_string(key_file)?;
    let signing_key: SigningKey = serde_json::from_str(&keypair_json)?;

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    {
        let mut pg_connection = diesel::pg::PgConnection::establish(&database_url)
            .expect("Failed to connect to database for migrations");
        db::run_migrations(&mut pg_connection).expect("Failed to run database migrations");
    }
    let ctx = graphql::BaseContext {
        grpc_client: tonic::transport::Channel::from_shared(
            std::env::var("MANAGER_ENDPOINT").expect("No manager endpoint set"),
        )
        .expect("Invalid manager endpoint URL")
        .connect()
        .await?,
        db_pool: {
            let manager =
                AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
            diesel_async::pooled_connection::bb8::Pool::builder()
                .build(manager)
                .await
                .expect("Failed to create DB connection pool")
        },
        keypair: signing_key,
    };
    tracing::info!("Listening on http://{addr}");
    loop {
        let (stream, remote_addr) = listener.accept().await?;

        let io = TokioIo::new(stream);

        let root_node = root_node.clone();
        let ctx = ctx.clone();

        tokio::spawn(async move {
            let root_node = root_node.clone();
            let ctx = ctx.clone();

            if let Err(e) = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                .serve_connection(
                    io,
                    service_fn(move |req| {
                        let root_node = root_node.clone();
                        let mut remote_ip = remote_addr.ip();

                        let is_private = match remote_ip {
                            std::net::IpAddr::V4(ipv4) => ipv4.is_private(),
                            std::net::IpAddr::V6(ipv6) => ipv6.is_unique_local(),
                        };

                        if is_private {
                            if let Some(xff) = req.headers().get("x-forwarded-for") {
                                if let Ok(xff_str) = xff.to_str() {
                                    for ip_str in xff_str.split(',') {
                                        if let Ok(ip) = ip_str.trim().parse::<std::net::IpAddr>() {
                                            let is_private = match ip {
                                                std::net::IpAddr::V4(ipv4) => ipv4.is_private(),
                                                std::net::IpAddr::V6(ipv6) => {
                                                    ipv6.is_unique_local()
                                                }
                                            };
                                            if !is_private {
                                                remote_ip = ip;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let auth = req.headers().get("authorization").and_then(|auth_header| {
                            let auth_str = auth_header.to_str().ok()?;
                            if auth_str.starts_with("Bearer ") {
                                Some(auth_str.trim_start_matches("Bearer ").to_string())
                            } else {
                                None
                            }
                        });
                        let user_details = auth
                            .and_then(|token| {
                                graphql::auth::parse_and_validate_jwt::<
                                        graphql::auth::AuthJwtPayload,
                                    >(
                                        &token, &ctx.keypair.verifying_key()
                                    )
                                    .ok()
                            })
                            .map(|jwt| AuthenticatedUser {
                                role: jwt.custom_fields.role,
                                username: jwt.custom_fields.username,
                                team_slug: jwt.custom_fields.team_slug,
                                user_id: jwt.sub,
                                team_id: jwt.custom_fields.team_id,
                            });

                        let ctx = Context::new(
                            ctx.clone(),
                            remote_ip,
                            req.headers()
                                .get("user-agent")
                                .and_then(|ua| ua.to_str().ok())
                                .unwrap_or("unknown")
                                .to_string(),
                            user_details,
                        );

                        async {
                            Ok::<_, Infallible>(match (req.method(), req.uri().path()) {
                                (&Method::GET, "/graphql") | (&Method::POST, "/graphql") => {
                                    graphql(root_node, Arc::new(ctx), req).await
                                }
                                (&Method::OPTIONS, "/graphql") => {
                                    let mut resp = Response::new(String::new());
                                    *resp.status_mut() = StatusCode::NO_CONTENT;
                                    resp
                                }
                                (&Method::GET, "/graphiql") => graphiql("/graphql", None).await,
                                (&Method::GET, "/playground") => playground("/graphql", None).await,
                                _ => {
                                    let mut resp = Response::new(String::new());
                                    *resp.status_mut() = StatusCode::NOT_FOUND;
                                    resp
                                }
                            })
                        }
                    }),
                )
                .await
            {
                tracing::error!("Error serving connection: {e}");
            }
        });
    }
}
