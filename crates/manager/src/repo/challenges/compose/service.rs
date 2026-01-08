// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use thiserror::Error;

mod deployment;
mod ingress;
mod service;
mod ssh;

#[derive(Error, Debug)]
pub enum ComposeServiceError {
    #[error("Anonymous volumes are not supported")]
    AnonymousVolume,
    #[error("Host paths outside of ./data/ are not supported: {0}")]
    HostPathVolume(String),
    #[error("Named pipe volumes are not supported")]
    NamedPipeVolume,
    #[error("Cluster volumes are not supported")]
    ClusterVolume,
    #[error("Ports with HostIP are not supported")]
    PortWithHostIp,
    #[error("User and group names are not supported")]
    UserNameNotSupported,
    #[error("References to env files outside of the working directory are not supported: {0}")]
    EnvFileOutOfBounds(String),
    #[error("Failed to read environment file {0}: {1}")]
    EnvFileReadError(String, std::io::Error),
    #[error("Failed to parse environment file {0}: {1}")]
    EnvFileParseError(String, String),
    #[error("Failed to read environment file {0} (line {1}): {2}")]
    EnvFileParseErrorDetailed(String, usize, String),
    #[error("Property not supported: {0}")]
    PropertyNotSupported(String),
    #[error("External volume not supported")]
    ExternalVolume,
    #[error("Other error: {0}")]
    Other(String),
}

pub trait AsDeployment {
    fn as_deployment(
        &self,
        id: String,
        working_dir: &Path,
    ) -> Result<k8s_openapi::api::apps::v1::Deployment, ComposeServiceError>;
    fn requires_data_pvc(&self) -> bool;
}

pub trait AsService {
    fn as_internal_svc(&self, id: String) -> k8s_openapi::api::core::v1::Service;
}

pub trait AsExternalService {
    fn as_proxied_svc(
        &self,
        id: String,
    ) -> Result<Option<k8s_openapi::api::core::v1::Service>, ComposeServiceError>;

    fn as_lb_svc(
        &self,
        id: String,
    ) -> Result<Option<k8s_openapi::api::core::v1::Service>, ComposeServiceError>;
}

pub trait AsIngress {
    fn as_http_ingress(
        &self,
        id: String,
        full_instance_name: &str,
        exposed_domain: &str,
    ) -> Result<Option<k8s_crds_traefik::IngressRoute>, ComposeServiceError>;

    fn as_tcp_ingress(
        &self,
        id: String,
        full_instance_name: &str,
        exposed_domain: &str,
    ) -> Result<Option<k8s_crds_traefik::IngressRouteTCP>, ComposeServiceError>;
}

pub trait AsSshGateway {
    fn as_ssh_gateways(
        &self,
        id: String,
        ssh_password: Option<String>,
    ) -> Result<Vec<crate::ssh::SSHGateway>, ComposeServiceError>;
}
