// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{repo::challenges::compose::service::ComposeServiceError, utils::split_with_quotes};

/// Builds the container spec from compose service configuration
pub fn build_container_spec(
    svc: &compose_spec::Service,
    id: String,
    env: Vec<k8s_openapi::api::core::v1::EnvVar>,
    volume_mounts: Vec<k8s_openapi::api::core::v1::VolumeMount>,
    security_context: Option<k8s_openapi::api::core::v1::SecurityContext>,
) -> Result<k8s_openapi::api::core::v1::Container, ComposeServiceError> {
    Ok(k8s_openapi::api::core::v1::Container {
        name: id,
        image: svc.image.as_ref().map(|i| i.to_string()),
        image_pull_policy: svc.pull_policy.as_ref().map(convert_pull_policy),
        stdin: Some(svc.stdin_open),
        tty: Some(svc.tty),
        working_dir: svc
            .working_dir
            .as_ref()
            .map(|p| p.as_path().to_string_lossy().to_string()),
        lifecycle: svc
            .stop_signal
            .as_ref()
            .map(|signal| k8s_openapi::api::core::v1::Lifecycle {
                stop_signal: Some(signal.clone()),
                ..Default::default()
            }),
        resources: build_resource_requirements(svc),
        ports: build_container_ports(svc),
        security_context,
        command: build_command(svc),
        args: build_args(svc),
        env: Some(env),
        volume_mounts: Some(volume_mounts),
        ..Default::default()
    })
}

fn convert_pull_policy(p: &compose_spec::service::PullPolicy) -> String {
    match p {
        compose_spec::service::PullPolicy::Always => "Always".to_string(),
        compose_spec::service::PullPolicy::Never => "Never".to_string(),
        compose_spec::service::PullPolicy::Missing => "IfNotPresent".to_string(),
        compose_spec::service::PullPolicy::Build => "IfNotPresent".to_string(), // fallback
    }
}

fn build_resource_requirements(
    svc: &compose_spec::Service,
) -> Option<k8s_openapi::api::core::v1::ResourceRequirements> {
    let mut requests = std::collections::BTreeMap::new();
    let mut limits = std::collections::BTreeMap::new();

    // Memory requests and limits
    if let Some(mem_res) = &svc.mem_reservation {
        requests.insert(
            "memory".to_string(),
            k8s_openapi::apimachinery::pkg::api::resource::Quantity(mem_res.to_string()),
        );
    }
    if let Some(mem_lim) = &svc.mem_limit {
        limits.insert(
            "memory".to_string(),
            k8s_openapi::apimachinery::pkg::api::resource::Quantity(mem_lim.to_string()),
        );
    }

    // CPU limits
    if let Some(cpus) = &svc.cpus {
        limits.insert(
            "cpu".to_string(),
            k8s_openapi::apimachinery::pkg::api::resource::Quantity(cpus.into_inner().to_string()),
        );
    } else if let Some(cpu_count) = svc.cpu_count {
        limits.insert(
            "cpu".to_string(),
            k8s_openapi::apimachinery::pkg::api::resource::Quantity(cpu_count.to_string()),
        );
    }

    if requests.is_empty() && limits.is_empty() {
        None
    } else {
        Some(k8s_openapi::api::core::v1::ResourceRequirements {
            requests: if requests.is_empty() {
                None
            } else {
                Some(requests)
            },
            limits: if limits.is_empty() {
                None
            } else {
                Some(limits)
            },
            ..Default::default()
        })
    }
}

fn build_container_ports(
    svc: &compose_spec::Service,
) -> Option<Vec<k8s_openapi::api::core::v1::ContainerPort>> {
    if svc.expose.is_empty() {
        None
    } else {
        Some(
            svc.expose
                .iter()
                .map(|expose| k8s_openapi::api::core::v1::ContainerPort {
                    container_port: expose.range.start() as i32,
                    protocol: Some(match expose.protocol {
                        Some(compose_spec::service::ports::Protocol::Tcp) | None => {
                            "TCP".to_string()
                        }
                        Some(compose_spec::service::ports::Protocol::Udp) => "UDP".to_string(),
                        Some(compose_spec::service::ports::Protocol::Other(ref s)) => s.clone(),
                    }),
                    ..Default::default()
                })
                .collect(),
        )
    }
}

fn build_command(svc: &compose_spec::Service) -> Option<Vec<String>> {
    if svc.init {
        // When init is true, wrap with tini
        Some(vec!["/tini/tini".to_string(), "--".to_string()])
    } else {
        svc.entrypoint.as_ref().map(|cmd| match cmd {
            compose_spec::service::Command::String(cmd) => split_with_quotes(cmd),
            compose_spec::service::Command::List(items) => items.clone(),
        })
    }
}

fn build_args(svc: &compose_spec::Service) -> Option<Vec<String>> {
    if svc.init {
        // When init is true, args need to include the original entrypoint + command
        let mut all_args = Vec::new();

        if let Some(entrypoint) = &svc.entrypoint {
            match entrypoint {
                compose_spec::service::Command::String(cmd) => {
                    all_args.extend(split_with_quotes(cmd));
                }
                compose_spec::service::Command::List(items) => {
                    all_args.extend(items.clone());
                }
            }
        }

        if let Some(command) = &svc.command {
            match command {
                compose_spec::service::Command::String(cmd) => {
                    all_args.extend(split_with_quotes(cmd));
                }
                compose_spec::service::Command::List(items) => {
                    all_args.extend(items.clone());
                }
            }
        }

        if all_args.is_empty() {
            None
        } else {
            Some(all_args)
        }
    } else {
        svc.command.as_ref().map(|cmd| match cmd {
            compose_spec::service::Command::String(cmd) => split_with_quotes(cmd),
            compose_spec::service::Command::List(items) => items.clone(),
        })
    }
}

/// Builds init containers for tini installation if needed
pub fn build_init_containers(
    svc: &compose_spec::Service,
) -> Option<Vec<k8s_openapi::api::core::v1::Container>> {
    if svc.init {
        Some(vec![k8s_openapi::api::core::v1::Container {
            name: "install-tini".to_string(),
            image: Some("krallin/ubuntu-tini:latest".to_string()),
            command: Some(vec![
                "cp".to_string(),
                "-v".to_string(),
                "/usr/bin/tini".to_string(),
                "/tini/tini".to_string(),
            ]),
            volume_mounts: Some(vec![k8s_openapi::api::core::v1::VolumeMount {
                name: "tini".to_string(),
                mount_path: "/tini".to_string(),
                ..Default::default()
            }]),
            ..Default::default()
        }])
    } else {
        None
    }
}
