// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{convert::Infallible, error::Error, net::SocketAddr, sync::Arc};

use diesel::Connection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use ed25519_dalek::SigningKey;
use http_body_util::Full;
use hyper::{Method, Response, StatusCode, body::Bytes, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use juniper::{EmptySubscription, RootNode};
use juniper_hyper::{graphiql, graphql, playground};
use slugify::slugify;
use tokio::net::TcpListener;

use plfanzen_api::db;
use plfanzen_api::graphql::{self, AuthenticatedUser, Context, Mutation, Query, Schema};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Set RUST_LOG to debug
    unsafe {
        std::env::set_var("RUST_LOG", "debug");
    }
    tracing_subscriber::fmt::init();
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to set AWS-LC-RS as default TLS provider");

    // This is required so the bot is shown as online on Discord
    // Check if the DISCORD_TOKEN env var is set
    if std::env::var("DISCORD_TOKEN").is_err() {
        tracing::warn!(
            "DISCORD_TOKEN environment variable is not set; Discord bot will not be started."
        );
    } else {
        let _bot_task = tokio::spawn(async move {
            plfanzen_api::discord::run_new_client().await.unwrap();
        });
    }

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

                        if is_private
                            && let Some(xff) = req.headers().get("x-forwarded-for")
                            && let Ok(xff_str) = xff.to_str()
                        {
                            for ip_str in xff_str.split(',') {
                                if let Ok(ip) = ip_str.trim().parse::<std::net::IpAddr>() {
                                    let is_private = match ip {
                                        std::net::IpAddr::V4(ipv4) => ipv4.is_private(),
                                        std::net::IpAddr::V6(ipv6) => ipv6.is_unique_local(),
                                    };
                                    if !is_private {
                                        remote_ip = ip;
                                        break;
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

                        let ctx = ctx.clone();
                        async move {
                            let ctx = Context::new(
                                ctx,
                                remote_ip,
                                req.headers()
                                    .get("user-agent")
                                    .and_then(|ua| ua.to_str().ok())
                                    .unwrap_or("unknown")
                                    .to_string(),
                                user_details,
                            )
                            .await;
                            Ok::<_, Infallible>(match (req.method(), req.uri().path()) {
                                (&Method::GET, "/graphql") | (&Method::POST, "/graphql") => {
                                    tokio::time::timeout(
                                        std::time::Duration::from_secs(30),
                                        graphql(root_node, Arc::new(ctx), req),
                                    )
                                    .await
                                    .map(|resp| {
                                        resp.map(|body| {
                                            Full::new(Bytes::copy_from_slice(body.as_bytes()))
                                        })
                                    })
                                    .unwrap_or_else(|_| {
                                        let mut resp = Response::new(Full::new(Bytes::from(
                                            "Request timed out",
                                        )));
                                        *resp.status_mut() = StatusCode::GATEWAY_TIMEOUT;
                                        resp
                                    })
                                }
                                (&Method::OPTIONS, _) => {
                                    let mut resp = Response::new(Full::new(Bytes::new()));
                                    *resp.status_mut() = StatusCode::NO_CONTENT;
                                    resp
                                }
                                (&Method::GET, "/graphiql") => graphiql("/graphql", None)
                                    .await
                                    .map(|body| Full::new(Bytes::from(body))),
                                (&Method::GET, "/playground") => playground("/graphql", None)
                                    .await
                                    .map(|body| Full::new(Bytes::from(body))),
                                (&Method::GET, path) => {
                                    if path.starts_with("/export-challenge/") {
                                        let challenge_id = path
                                            .trim_start_matches("/export-challenge/")
                                            .to_string();
                                        let challenge_slug = slugify!(&challenge_id);
                                        match graphql::export_challenge(ctx, challenge_id.clone())
                                            .await
                                        {
                                            Ok(archive_data) => {
                                                let mut resp = Response::new(Full::new(
                                                    Bytes::from(archive_data),
                                                ));
                                                resp.headers_mut().insert(
                                                    hyper::header::CONTENT_TYPE,
                                                    hyper::header::HeaderValue::from_static(
                                                        "application/gzip",
                                                    ),
                                                );
                                                let filename = format!("{}.tar.gz", challenge_slug);
                                                resp.headers_mut().insert(
                                                    hyper::header::CONTENT_DISPOSITION,
                                                    hyper::header::HeaderValue::from_str(&format!(
                                                        "attachment; filename=\"{}\"",
                                                        filename
                                                    ))
                                                    .unwrap(),
                                                );
                                                resp
                                            }
                                            Err((status_code, message)) => {
                                                let mut resp =
                                                    Response::new(Full::new(Bytes::from(message)));
                                                *resp.status_mut() = StatusCode::from_u16(
                                                    status_code,
                                                )
                                                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                                                resp
                                            }
                                        }
                                    } else if path.starts_with("/retrieve-file/") {
                                        let parts: Vec<&str> = path
                                            .trim_start_matches("/retrieve-file/")
                                            .splitn(2, '/')
                                            .collect();
                                        if parts.len() != 2 {
                                            let mut resp = Response::new(Full::new(Bytes::from(
                                                "Invalid request",
                                            )));
                                            *resp.status_mut() = StatusCode::BAD_REQUEST;
                                            return Ok(resp);
                                        }
                                        let challenge_id = parts[0].to_string();
                                        let filename = parts[1].to_string();
                                        match graphql::retrieve_file(
                                            ctx,
                                            challenge_id.clone(),
                                            filename.clone(),
                                        )
                                        .await
                                        {
                                            Ok(file_data) => {
                                                let mut resp = Response::new(Full::new(
                                                    Bytes::from(file_data),
                                                ));
                                                resp.headers_mut().insert(
                                                    hyper::header::CONTENT_TYPE,
                                                    hyper::header::HeaderValue::from_static(
                                                        "application/octet-stream",
                                                    ),
                                                );
                                                let file_slug = slugify!(&filename);
                                                let content_disposition = format!(
                                                    "attachment; filename=\"{}\"",
                                                    file_slug
                                                );
                                                resp.headers_mut().insert(
                                                    hyper::header::CONTENT_DISPOSITION,
                                                    hyper::header::HeaderValue::from_str(
                                                        &content_disposition,
                                                    )
                                                    .unwrap(),
                                                );
                                                resp
                                            }
                                            Err((status_code, message)) => {
                                                let mut resp =
                                                    Response::new(Full::new(Bytes::from(message)));
                                                *resp.status_mut() = StatusCode::from_u16(
                                                    status_code,
                                                )
                                                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                                                resp
                                            }
                                        }
                                    } else {
                                        let mut resp = Response::new(Full::new(Bytes::new()));
                                        *resp.status_mut() = StatusCode::NOT_FOUND;
                                        resp
                                    }
                                }
                                _ => {
                                    let mut resp = Response::new(Full::new(Bytes::new()));
                                    *resp.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
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
