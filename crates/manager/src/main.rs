// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use crate::grpc::{
    ChallengeManager, ChallengesServiceServer, RepoManager, RepositoryServiceServer,
};

mod grpc;
mod instances;
mod js;
mod repo;
mod ssh;
mod utils;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    rustls::crypto::aws_lc_rs::default_provider().install_default().expect("Failed to set AWS-LC-RS as default TLS provider");
    let kube_client = kube::Client::try_default()
        .await
        .expect("Failed to create kube client");
    let challenge_manager = ChallengeManager {
        repo_dir: PathBuf::from(std::env::var("REPO_DIR").unwrap_or_else(|_| "/data/repo".into())),
        kube_client,
    };
    let repo_manager = RepoManager {
        repo_dir: PathBuf::from(std::env::var("REPO_DIR").unwrap_or_else(|_| "/data/repo".into())),
        git_url: std::env::var("GIT_URL").expect("GIT_URL must be set"),
        git_branch: std::env::var("GIT_BRANCH").expect("GIT_BRANCH must be set"),
    };
    let addr = "[::]:50051".parse().unwrap();
    println!("Plfanzen manager listening on {}", addr);
    tonic::transport::Server::builder()
        .add_service(ChallengesServiceServer::new(challenge_manager))
        .add_service(RepositoryServiceServer::new(repo_manager))
        .serve(addr)
        .await
        .unwrap();
}
