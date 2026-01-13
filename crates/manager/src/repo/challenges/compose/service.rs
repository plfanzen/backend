// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeMap, path::Path};

use thiserror::Error;

use crate::repo::challenges::{compose::service::networking::HasNetworkPolicy, vm::VirtualMachine};

mod deployment;
mod ingress;
pub mod networking;
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

pub trait HasPorts {
    fn get_ports(&self) -> &compose_spec::service::ports::Ports;
}

trait HasPortHelpers {
    fn is_empty(&self) -> bool;
    fn long_iter_clone(&self) -> impl Iterator<Item = compose_spec::service::ports::Port> + '_;
}

impl<T: HasPorts> HasPortHelpers for T {
    fn is_empty(&self) -> bool {
        self.get_ports().is_empty()
    }

    fn long_iter_clone(&self) -> impl Iterator<Item = compose_spec::service::ports::Port> + '_ {
        compose_spec::service::ports::into_long_iter(self.get_ports().clone())
    }
}

pub trait AsExternalService {
    fn as_proxied_svc(
        &self,
        id: String,
        labels: Option<BTreeMap<String, String>>,
    ) -> Result<Option<k8s_openapi::api::core::v1::Service>, ComposeServiceError>;

    fn as_lb_svc(
        &self,
        id: String,
        labels: Option<BTreeMap<String, String>>,
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

pub trait HasLabels {
    fn get_labels(&self, id: &str) -> BTreeMap<String, String>;
}

// TODO: Move to the proper place
impl HasLabels for compose_spec::service::Service {
    fn get_labels(&self, id: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("compose-service-id".to_string(), id.to_string()),
            (
                "networkpolicy".to_string(),
                if self.get_network_policy().is_some() {
                    format!("svc-{}", id)
                } else {
                    "base".to_string()
                },
            ),
        ])
    }
}

impl HasLabels for VirtualMachine {
    fn get_labels(&self, id: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("virtual-machine-id".to_string(), id.to_string()),
            (
                "networkpolicy".to_string(),
                if self.get_network_policy().is_some() {
                    format!("virtual-machine-{}", id)
                } else {
                    "base".to_string()
                },
            ),
        ])
    }
}
