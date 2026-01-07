// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod container;
mod environment;
mod security;
mod validation;
mod volumes;

use std::path::Path;

use kube::api::ObjectMeta;

use crate::repo::challenges::compose::service::{AsDeployment, ComposeServiceError};

impl AsDeployment for compose_spec::Service {
    fn as_deployment(
        &self,
        id: String,
        working_dir: &Path,
    ) -> Result<k8s_openapi::api::apps::v1::Deployment, ComposeServiceError> {
        validation::ensure_only_supported(self)?;
        
        let working_dir = working_dir.canonicalize().map_err(|e| {
            ComposeServiceError::Other(format!(
                "Failed to canonicalize working directory {}: {}",
                working_dir.to_string_lossy(),
                e
            ))
        })?;

        let env = environment::process_environment(self, &working_dir)?;
        let replicas = calculate_replicas(self)?;
        Ok(k8s_openapi::api::apps::v1::Deployment {
            metadata: ObjectMeta {
                name: Some(id.clone()),
                labels: extract_deploy_labels(self),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::apps::v1::DeploymentSpec {
                replicas,
                selector: k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector {
                    match_labels: Some(
                        [("component".to_string(), id.clone())]
                            .iter()
                            .cloned()
                            .collect(),
                    ),
                    ..Default::default()
                },
                template: k8s_openapi::api::core::v1::PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: Some(build_pod_labels(self, &id)),
                        annotations: extract_annotations(self),
                        ..Default::default()
                    }),
                    spec: Some(build_pod_spec(self, id, env)?),
                },
                ..Default::default()
            }),
            status: None,
        })
    }

    fn requires_data_pvc(&self) -> bool {
        for vol in compose_spec::service::volumes::into_long_iter(self.volumes.clone()) {
            if let compose_spec::service::volumes::Mount::Bind(b) = vol {
                let host_path = b.source.as_inner();
                if host_path.starts_with("./data/") {
                    return true;
                }
            }
        }
        false
    }
}

fn calculate_replicas(
    svc: &compose_spec::Service,
) -> Result<Option<i32>, ComposeServiceError> {
    let mut replicas = svc.scale.map(|s| s as i32);
    if let Some(deploy_conf) = &svc.deploy {
        if let Some(deploy_replicas) = deploy_conf.replicas {
            if replicas.is_some_and(|r| r != (deploy_replicas as i32)) {
                return Err(ComposeServiceError::Other(
                    "Conflict between top-level scale and deploy.replicas".to_string(),
                ));
            }
            replicas = Some(deploy_replicas as i32);
        }
    }
    Ok(replicas)
}

fn extract_deploy_labels(
    svc: &compose_spec::Service,
) -> Option<std::collections::BTreeMap<String, String>> {
    svc.deploy
        .as_ref()
        .and_then(|d| d.labels.clone().into_map().ok())
        .and_then(|map| {
            if map.is_empty() {
                None
            } else {
                Some(
                    map.into_iter()
                        .filter_map(|(k, v)| Some((k.to_string(), v?.to_string())))
                        .collect(),
                )
            }
        })
}

fn build_pod_labels(
    svc: &compose_spec::Service,
    id: &str,
) -> std::collections::BTreeMap<String, String> {
    [("component".to_string(), id.to_string())]
        .iter()
        .cloned()
        .chain(
            svc.labels
                .clone()
                .into_map()
                .ok()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(k, v)| Some((k.to_string(), v?.to_string()))),
        )
        .collect()
}

fn extract_annotations(
    svc: &compose_spec::Service,
) -> Option<std::collections::BTreeMap<String, String>> {
    svc.annotations.clone().into_map().ok().and_then(|map| {
        if map.is_empty() {
            None
        } else {
            Some(
                map.into_iter()
                    .filter_map(|(k, v)| Some((k.to_string(), v?.to_string())))
                    .collect(),
            )
        }
    })
}

fn build_pod_spec(
    svc: &compose_spec::Service,
    id: String,
    env: Vec<k8s_openapi::api::core::v1::EnvVar>,
) -> Result<k8s_openapi::api::core::v1::PodSpec, ComposeServiceError> {
    let volumes = volumes::build_volumes(svc)?;
    let volume_mounts = volumes::build_volume_mounts(svc)?;
    let security_context = security::build_container_security_context(svc)?;
    let container = container::build_container_spec(svc, id, env, volume_mounts, security_context)?;

    Ok(k8s_openapi::api::core::v1::PodSpec {
        runtime_class_name: if svc.privileged || !svc.cap_add.is_empty() {
            Some("kata".to_string())
        } else {
            svc.runtime.clone()
        },
        hostname: svc.hostname.as_ref().map(|h| h.to_string()),
        subdomain: svc.domain_name.as_ref().map(|d| d.to_string()),
        host_aliases: build_host_aliases(svc),
        dns_config: build_dns_config(svc),
        termination_grace_period_seconds: svc.stop_grace_period.as_ref().map(|d| d.as_secs() as i64),
        volumes: Some(volumes),
        os: Some(k8s_openapi::api::core::v1::PodOS {
            // Otherwise, stop_signal can not be used
            name: "linux".to_string(),
            ..Default::default()
        }),
        init_containers: container::build_init_containers(svc),
        security_context: security::build_pod_security_context(svc),
        containers: vec![container],
        ..Default::default()
    })
}

fn build_host_aliases(
    svc: &compose_spec::Service,
) -> Option<Vec<k8s_openapi::api::core::v1::HostAlias>> {
    if svc.extra_hosts.is_empty() {
        None
    } else {
        Some(
            svc.extra_hosts
                .iter()
                .map(|(hostname, ip)| k8s_openapi::api::core::v1::HostAlias {
                    hostnames: Some(vec![hostname.to_string()]),
                    ip: ip.to_string(),
                })
                .collect(),
        )
    }
}

fn build_dns_config(
    svc: &compose_spec::Service,
) -> Option<k8s_openapi::api::core::v1::PodDNSConfig> {
    let has_dns = svc.dns.is_some() || !svc.dns_opt.is_empty() || svc.dns_search.is_some();
    if !has_dns {
        None
    } else {
        Some(k8s_openapi::api::core::v1::PodDNSConfig {
            nameservers: svc.dns.as_ref().map(|dns| match dns {
                compose_spec::ItemOrList::Item(ip) => vec![ip.to_string()],
                compose_spec::ItemOrList::List(ips) => {
                    ips.iter().map(|ip| ip.to_string()).collect()
                }
            }),
            searches: svc.dns_search.as_ref().map(|dns_search| match dns_search {
                compose_spec::ItemOrList::Item(h) => vec![h.to_string()],
                compose_spec::ItemOrList::List(hs) => hs.iter().map(|h| h.to_string()).collect(),
            }),
            options: if svc.dns_opt.is_empty() {
                None
            } else {
                Some(
                    svc.dns_opt
                        .iter()
                        .map(|opt| k8s_openapi::api::core::v1::PodDNSConfigOption {
                            name: Some(opt.clone()),
                            ..Default::default()
                        })
                        .collect(),
                )
            },
            ..Default::default()
        })
    }
}
