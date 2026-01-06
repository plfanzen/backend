// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use compose_spec::service::IdOrName;
use kube::api::ObjectMeta;

use crate::{
    repo::challenges::compose::service::{AsDeployment, ComposeServiceError},
    utils::split_with_quotes,
};

use slugify::slugify;

macro_rules! ensure_option_none {
    ($field:expr) => {
        if $field.is_some() {
            return Err(ComposeServiceError::PropertyNotSupported(
                stringify!($field).to_string(),
            ));
        }
    };
}

macro_rules! ensure_map_empty {
    ($field:expr) => {
        if !$field.is_empty() {
            return Err(ComposeServiceError::PropertyNotSupported(
                stringify!($field).to_string(),
            ));
        }
    };
}

macro_rules! ensure_false {
    ($field:expr) => {
        if $field {
            return Err(ComposeServiceError::PropertyNotSupported(
                stringify!($field).to_string(),
            ));
        }
    };
}

fn ensure_only_supported(svc: &compose_spec::Service) -> Result<(), ComposeServiceError> {
    ensure_option_none!(svc.build);
    ensure_map_empty!(svc.storage_opt);
    ensure_map_empty!(svc.sysctls);
    ensure_map_empty!(svc.ulimits);
    ensure_option_none!(svc.mem_swappiness);
    ensure_option_none!(svc.memswap_limit);
    ensure_option_none!(svc.pid);
    ensure_option_none!(svc.pids_limit);
    ensure_option_none!(svc.network_config);
    ensure_option_none!(svc.mac_address);
    ensure_false!(svc.oom_kill_disable);
    ensure_option_none!(svc.oom_score_adj);
    ensure_option_none!(svc.platform);
    ensure_map_empty!(svc.security_opt);
    ensure_map_empty!(svc.profiles);
    Ok(())
}

impl AsDeployment for compose_spec::Service {
    fn as_deployment(
        &self,
        id: String,
        working_dir: &Path,
    ) -> Result<k8s_openapi::api::apps::v1::Deployment, ComposeServiceError> {
        ensure_only_supported(self)?;
        let working_dir = working_dir.canonicalize().map_err(|e| {
            ComposeServiceError::Other(format!(
                "Failed to canonicalize working directory {}: {}",
                working_dir.to_string_lossy(),
                e
            ))
        })?;
        let mut env: Vec<_> = self
            .environment
            .clone()
            .into_map()
            .map_err(|e| ComposeServiceError::Other(e.to_string()))?
            .into_iter()
            .map(|(k, v)| k8s_openapi::api::core::v1::EnvVar {
                name: k.to_string(),
                value: v.map(|val| val.to_string()),
                ..Default::default()
            })
            .collect();
        if let Some(env_file) = &self.env_file {
            for file in env_file.clone().into_list() {
                let file = file.into_long();
                if file.path.is_absolute() {
                    return Err(ComposeServiceError::EnvFileOutOfBounds(
                        file.path.to_string_lossy().to_string(),
                    ));
                }
                let abs_path = working_dir.join(file.path);
                match abs_path.canonicalize() {
                    Err(e) => {
                        if file.required {
                            return Err(ComposeServiceError::Other(format!(
                                "Failed to canonicalize env_file path {}: {}",
                                abs_path.to_string_lossy(),
                                e
                            )));
                        } else {
                            continue;
                        }
                    }
                    Ok(canonical_path) => {
                        if !canonical_path.starts_with(&working_dir) {
                            return Err(ComposeServiceError::EnvFileOutOfBounds(
                                canonical_path.to_string_lossy().to_string(),
                            ));
                        }
                        let parsed = match dotenvy::from_path_iter(&canonical_path) {
                            Ok(iter) => iter,
                            Err(e) => {
                                if file.required {
                                    return Err(ComposeServiceError::EnvFileReadError(
                                        canonical_path.to_string_lossy().to_string(),
                                        match e {
                                            dotenvy::Error::Io(io_err) => io_err,
                                            // Should be unreachable, but handle just in case
                                            other => std::io::Error::new(
                                                std::io::ErrorKind::Other,
                                                other.to_string(),
                                            ),
                                        },
                                    ));
                                } else {
                                    continue;
                                }
                            }
                        };
                        for item in parsed {
                            match item {
                                Ok((key, value)) => {
                                    env.push(k8s_openapi::api::core::v1::EnvVar {
                                        name: key,
                                        value: Some(value),
                                        ..Default::default()
                                    });
                                }
                                Err(e) => {
                                    if file.required {
                                        match e {
                                            dotenvy::Error::LineParse(line, line_no) => {
                                                return Err(
                                                    ComposeServiceError::EnvFileParseErrorDetailed(
                                                        canonical_path
                                                            .to_string_lossy()
                                                            .to_string(),
                                                        line_no,
                                                        line,
                                                    ),
                                                );
                                            }
                                            dotenvy::Error::Io(error) => {
                                                return Err(ComposeServiceError::EnvFileReadError(
                                                    canonical_path.to_string_lossy().to_string(),
                                                    error,
                                                ));
                                            }
                                            dotenvy::Error::EnvVar(var_error) => {
                                                return Err(
                                                    ComposeServiceError::EnvFileParseError(
                                                        canonical_path
                                                            .to_string_lossy()
                                                            .to_string(),
                                                        var_error.to_string(),
                                                    ),
                                                );
                                            }
                                            _ => {
                                                return Err(
                                                    ComposeServiceError::EnvFileParseError(
                                                        canonical_path
                                                            .to_string_lossy()
                                                            .to_string(),
                                                        e.to_string(),
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut replicas = self.scale.map(|s| s as i32);
        if let Some(deploy_conf) = &self.deploy {
            if let Some(deploy_replicas) = deploy_conf.replicas {
                if replicas.is_some_and(|r| r != (deploy_replicas as i32)) {
                    // Conflict between top-level scale and deploy.replicas
                    return Err(ComposeServiceError::Other(
                        "Conflict between top-level scale and deploy.replicas".to_string(),
                    ));
                }
                replicas = Some(deploy_replicas as i32);
            }
        }
        Ok(k8s_openapi::api::apps::v1::Deployment {
            metadata: ObjectMeta {
                name: Some(id.clone()),
                labels: self
                    .deploy
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
                    }),
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
                        labels: Some(
                            [("component".to_string(), id.clone())]
                                .iter()
                                .cloned()
                                .chain(
                                    self.labels
                                        .clone()
                                        .into_map()
                                        .ok()
                                        .unwrap_or_default()
                                        .into_iter()
                                        .filter_map(|(k, v)| Some((k.to_string(), v?.to_string()))),
                                )
                                .collect(),
                        ),
                        annotations: self.annotations.clone().into_map().ok().and_then(|map| {
                            if map.is_empty() {
                                None
                            } else {
                                Some(
                                    map.into_iter()
                                        .filter_map(|(k, v)| Some((k.to_string(), v?.to_string())))
                                        .collect(),
                                )
                            }
                        }),
                        ..Default::default()
                    }),
                    spec: Some(k8s_openapi::api::core::v1::PodSpec {
                        runtime_class_name: if self.privileged || !self.cap_add.is_empty() {
                            Some("kata".to_string())
                        } else {
                            self.runtime.clone()
                        },
                        hostname: self.hostname.as_ref().map(|h| h.to_string()),
                        subdomain: self.domain_name.as_ref().map(|d| d.to_string()),
                        host_aliases: if self.extra_hosts.is_empty() {
                            None
                        } else {
                            Some(
                                self.extra_hosts
                                    .iter()
                                    .map(|(hostname, ip)| k8s_openapi::api::core::v1::HostAlias {
                                        hostnames: Some(vec![hostname.to_string()]),
                                        ip: ip.to_string(),
                                    })
                                    .collect(),
                            )
                        },
                        dns_config: {
                            let has_dns = self.dns.is_some()
                                || !self.dns_opt.is_empty()
                                || self.dns_search.is_some();
                            if !has_dns {
                                None
                            } else {
                                Some(k8s_openapi::api::core::v1::PodDNSConfig {
                                    nameservers: self.dns.as_ref().map(|dns| match dns {
                                        compose_spec::ItemOrList::Item(ip) => vec![ip.to_string()],
                                        compose_spec::ItemOrList::List(ips) => {
                                            ips.iter().map(|ip| ip.to_string()).collect()
                                        }
                                    }),
                                    searches: self.dns_search.as_ref().map(|dns_search| {
                                        match dns_search {
                                            compose_spec::ItemOrList::Item(h) => {
                                                vec![h.to_string()]
                                            }
                                            compose_spec::ItemOrList::List(hs) => {
                                                hs.iter().map(|h| h.to_string()).collect()
                                            }
                                        }
                                    }),
                                    options: if self.dns_opt.is_empty() {
                                        None
                                    } else {
                                        Some(
                                            self.dns_opt
                                                .iter()
                                                .map(|opt| {
                                                    k8s_openapi::api::core::v1::PodDNSConfigOption {
                                                        name: Some(opt.clone()),
                                                        ..Default::default()
                                                    }
                                                })
                                                .collect(),
                                        )
                                    },
                                    ..Default::default()
                                })
                            }
                        },
                        termination_grace_period_seconds: self
                            .stop_grace_period
                            .as_ref()
                            .map(|d| d.as_secs() as i64),
                        // We ignore restart_policy, because Kubernetes only allows Always for Deployments
                        volumes: Some({
                            let mut volumes: Vec<k8s_openapi::api::core::v1::Volume> =
                                compose_spec::service::volumes
                                    ::into_long_iter(self.volumes.clone())
                                    .map(|vol| {
                                        match vol {
                                            compose_spec::service::volumes::Mount::Volume(
                                                volume,
                                            ) => {
                                                let vol_name = volume.source
                                                    .as_ref()
                                                    .ok_or(ComposeServiceError::AnonymousVolume)?
                                                    .clone();
                                                Ok(k8s_openapi::api::core::v1::Volume {
                                                    name: vol_name.to_string(),
                                                    persistent_volume_claim: Some(
                                                        k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                                                            claim_name: vol_name.to_string(),
                                                            ..Default::default()
                                                        }
                                                    ),
                                                    ..Default::default()
                                                })
                                            }
                                            compose_spec::service::volumes::Mount::Bind(b) => {
                                                let host_path = b.source.as_inner();
                                                if !host_path.starts_with("./data/") {
                                                    return Err(
                                                        ComposeServiceError::HostPathVolume(
                                                            host_path.to_string_lossy().to_string()
                                                        )
                                                    );
                                                }
                                                let pvc_name = host_path
                                                    .strip_prefix("./data/")
                                                    .ok()
                                                    .and_then(|p| p.components().next())
                                                    .ok_or_else(||
                                                        ComposeServiceError::HostPathVolume(
                                                            host_path.to_string_lossy().to_string()
                                                        )
                                                    )?
                                                    .as_os_str()
                                                    .to_string_lossy()
                                                    .to_string();
                                                Ok(k8s_openapi::api::core::v1::Volume {
                                                    name: slugify!(
                                                        &b.common.target
                                                            .as_inner()
                                                            .to_string_lossy()
                                                    ),
                                                    persistent_volume_claim: Some(
                                                        k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                                                            claim_name: pvc_name,
                                                            ..Default::default()
                                                        }
                                                    ),
                                                    ..Default::default()
                                                })
                                            }
                                            compose_spec::service::volumes::Mount::Tmpfs(tmpfs) =>
                                                Ok(k8s_openapi::api::core::v1::Volume {
                                                    name: slugify!(
                                                        &tmpfs.common.target
                                                            .as_inner()
                                                            .to_string_lossy()
                                                    ),
                                                    empty_dir: Some(
                                                        k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                                                            medium: Some("Memory".to_string()),
                                                            ..Default::default()
                                                        }
                                                    ),
                                                    ..Default::default()
                                                }),
                                            compose_spec::service::volumes::Mount::NamedPipe(_) =>
                                                Err(ComposeServiceError::NamedPipeVolume),
                                            compose_spec::service::volumes::Mount::Cluster(_) =>
                                                Err(ComposeServiceError::ClusterVolume),
                                        }
                                    })

                                    .collect::<Result<Vec<_>, ComposeServiceError>>()?;

                            // Add /dev/shm volume if shm_size is specified
                            if let Some(shm_size) = &self.shm_size {
                                volumes.push(k8s_openapi::api::core::v1::Volume {
                                    name: "dshm".to_string(),
                                    empty_dir: Some(
                                        k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                                            medium: Some("Memory".to_string()),
                                            size_limit: Some(
                                                k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                                    shm_size.to_string()
                                                )
                                            ),
                                        }
                                    ),
                                    ..Default::default()
                                });
                            }

                            if let Some(tmpfs_mounts) = &self.tmpfs {
                                for item in tmpfs_mounts.clone().into_list() {
                                    let mount_path = item.as_inner();
                                    volumes.push(k8s_openapi::api::core::v1::Volume {
                                        name: slugify!(&mount_path.to_string_lossy()),
                                        empty_dir: Some(
                                            k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                                                medium: Some("Memory".to_string()),
                                                ..Default::default()
                                            },
                                        ),
                                        ..Default::default()
                                    });
                                }
                            }

                            // Add tini volume if init is true
                            if self.init {
                                volumes.push(k8s_openapi::api::core::v1::Volume {
                                    name: "tini".to_string(),
                                    empty_dir: Some(
                                        k8s_openapi::api::core::v1::EmptyDirVolumeSource::default(),
                                    ),
                                    ..Default::default()
                                });
                            }

                            volumes
                        }),
                        os: Some(k8s_openapi::api::core::v1::PodOS {
                            // Otherwise, stop_signal can not be used
                            name: "linux".to_string(),
                            ..Default::default()
                        }),
                        init_containers: if self.init {
                            Some(vec![k8s_openapi::api::core::v1::Container {
                                name: "install-tini".to_string(),
                                image: Some("krallin/ubuntu-tini:latest".to_string()),
                                command: Some(vec![
                                    "cp".to_string(),
                                    "-v".to_string(),
                                    "/usr/bin/tini".to_string(),
                                    "/tini/tini".to_string(),
                                ]),
                                volume_mounts: Some(vec![
                                    k8s_openapi::api::core::v1::VolumeMount {
                                        name: "tini".to_string(),
                                        mount_path: "/tini".to_string(),
                                        ..Default::default()
                                    },
                                ]),
                                ..Default::default()
                            }])
                        } else {
                            None
                        },
                        security_context: {
                            let mut pod_sec_ctx =
                                k8s_openapi::api::core::v1::PodSecurityContext::default();
                            let mut has_context = false;

                            // Supplemental groups from group_add
                            if !self.group_add.is_empty() {
                                let mut groups: Vec<i64> = Vec::new();
                                for group in &self.group_add {
                                    if let IdOrName::Id(gid) = group {
                                        groups.push(*gid as i64);
                                    } else if group.as_name().is_some_and(|n| n == "root") {
                                        groups.push(0);
                                    } else {
                                        return Err(ComposeServiceError::Other(
                                            "Group names are not supported in 'group_add' field"
                                                .to_string(),
                                        ));
                                    }
                                }
                                pod_sec_ctx.supplemental_groups = Some(groups);
                                has_context = true;
                            }

                            if has_context { Some(pod_sec_ctx) } else { None }
                        },
                        containers: vec![k8s_openapi::api::core::v1::Container {
                            name: id,
                            image: self.image.as_ref().map(|i| i.to_string()),
                            image_pull_policy: self.pull_policy.as_ref().map(|p| {
                                match p {
                                    compose_spec::service::PullPolicy::Always => {
                                        "Always".to_string()
                                    }
                                    compose_spec::service::PullPolicy::Never => "Never".to_string(),
                                    compose_spec::service::PullPolicy::Missing => {
                                        "IfNotPresent".to_string()
                                    }
                                    compose_spec::service::PullPolicy::Build => {
                                        "IfNotPresent".to_string()
                                    } // fallback
                                }
                            }),
                            stdin: Some(self.stdin_open),
                            tty: Some(self.tty),
                            working_dir: self
                                .working_dir
                                .as_ref()
                                .map(|p| p.as_path().to_string_lossy().to_string()),
                            lifecycle: self.stop_signal.as_ref().map(|signal| {
                                k8s_openapi::api::core::v1::Lifecycle {
                                    stop_signal: Some(signal.clone()),
                                    ..Default::default()
                                }
                            }),
                            resources: {
                                let mut requests = std::collections::BTreeMap::new();
                                let mut limits = std::collections::BTreeMap::new();

                                // Memory requests and limits
                                if let Some(mem_res) = &self.mem_reservation {
                                    requests.insert(
                                        "memory".to_string(),
                                        k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                            mem_res.to_string(),
                                        ),
                                    );
                                }
                                if let Some(mem_lim) = &self.mem_limit {
                                    limits.insert(
                                        "memory".to_string(),
                                        k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                            mem_lim.to_string(),
                                        ),
                                    );
                                }

                                // CPU limits
                                if let Some(cpus) = &self.cpus {
                                    limits.insert(
                                        "cpu".to_string(),
                                        k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                            cpus.into_inner().to_string(),
                                        ),
                                    );
                                } else if let Some(cpu_count) = self.cpu_count {
                                    limits.insert(
                                        "cpu".to_string(),
                                        k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                            cpu_count.to_string(),
                                        ),
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
                            },
                            ports: if self.expose.is_empty() {
                                None
                            } else {
                                Some(
                                    self.expose
                                        .iter()
                                        .map(|expose| k8s_openapi::api::core::v1::ContainerPort {
                                            container_port: expose.range.start() as i32,
                                            protocol: Some(match expose.protocol {
                                                Some(
                                                    compose_spec::service::ports::Protocol::Tcp,
                                                )
                                                | None => "TCP".to_string(),
                                                Some(
                                                    compose_spec::service::ports::Protocol::Udp,
                                                ) => "UDP".to_string(),
                                                Some(
                                                    compose_spec::service::ports::Protocol::Other(
                                                        ref s,
                                                    ),
                                                ) => s.clone(),
                                            }),
                                            ..Default::default()
                                        })
                                        .collect(),
                                )
                            },
                            security_context: {
                                let mut ctx =
                                    k8s_openapi::api::core::v1::SecurityContext::default();
                                let mut has_context = false;

                                if self.privileged {
                                    ctx.privileged = Some(true);
                                    has_context = true;
                                }

                                if let Some(user) = &self.user {
                                    // Parse user string (format: "uid[:gid]")
                                    if let IdOrName::Id(uid) = user.user {
                                        ctx.run_as_user = Some(uid as i64);
                                        has_context = true;
                                    } else if user.user.as_name().is_some_and(|n| n == "root") {
                                        ctx.run_as_user = Some(0);
                                        has_context = true;
                                    } else {
                                        return Err(ComposeServiceError::UserNameNotSupported);
                                    }
                                }

                                if self.read_only {
                                    ctx.read_only_root_filesystem = Some(true);
                                    has_context = true;
                                }

                                if !self.cap_add.is_empty() {
                                    let add_caps: Vec<String> =
                                        self.cap_add.iter().map(|cap| cap.to_string()).collect();
                                    ctx.capabilities =
                                        Some(k8s_openapi::api::core::v1::Capabilities {
                                            add: Some(add_caps),
                                            ..Default::default()
                                        });
                                    has_context = true;
                                }

                                if !self.cap_drop.is_empty() {
                                    let drop_caps: Vec<String> =
                                        self.cap_drop.iter().map(|cap| cap.to_string()).collect();
                                    if ctx.capabilities.is_none() {
                                        ctx.capabilities =
                                            Some(k8s_openapi::api::core::v1::Capabilities {
                                                drop: Some(drop_caps),
                                                ..Default::default()
                                            });
                                    } else if let Some(capabilities) = &mut ctx.capabilities {
                                        capabilities.drop = Some(drop_caps);
                                    }
                                    has_context = true;
                                }

                                if has_context { Some(ctx) } else { None }
                            },
                            command: if self.init {
                                // When init is true, wrap with tini
                                Some(vec!["/tini/tini".to_string(), "--".to_string()])
                            } else {
                                self.entrypoint.as_ref().map(|cmd| match cmd {
                                    compose_spec::service::Command::String(cmd) => {
                                        split_with_quotes(cmd)
                                    }
                                    compose_spec::service::Command::List(items) => items.clone(),
                                })
                            },
                            args: if self.init {
                                // When init is true, args need to include the original entrypoint + command
                                let mut all_args = Vec::new();

                                if let Some(entrypoint) = &self.entrypoint {
                                    match entrypoint {
                                        compose_spec::service::Command::String(cmd) => {
                                            all_args.extend(split_with_quotes(cmd));
                                        }
                                        compose_spec::service::Command::List(items) => {
                                            all_args.extend(items.clone());
                                        }
                                    }
                                }

                                if let Some(command) = &self.command {
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
                                self.command.as_ref().map(|cmd| match cmd {
                                    compose_spec::service::Command::String(cmd) => {
                                        split_with_quotes(cmd)
                                    }
                                    compose_spec::service::Command::List(items) => items.clone(),
                                })
                            },
                            env: Some(env),
                            volume_mounts: Some({
                                let mut mounts: Vec<k8s_openapi::api::core::v1::VolumeMount> =
                                    compose_spec::service::volumes::into_long_iter(
                                        self.volumes.clone(),
                                    )
                                    .map(|vol| match vol {
                                        compose_spec::service::volumes::Mount::Volume(volume) => {
                                            let vol_name = volume
                                                .source
                                                .as_ref()
                                                .ok_or(ComposeServiceError::AnonymousVolume)?
                                                .clone();
                                            Ok(k8s_openapi::api::core::v1::VolumeMount {
                                                name: vol_name.to_string(),
                                                mount_path: volume
                                                    .common
                                                    .target
                                                    .as_inner()
                                                    .to_string_lossy()
                                                    .to_string(),
                                                ..Default::default()
                                            })
                                        }
                                        compose_spec::service::volumes::Mount::Bind(b) => {
                                            Ok(k8s_openapi::api::core::v1::VolumeMount {
                                                name: slugify!(
                                                    &b.common.target.as_inner().to_string_lossy()
                                                ),
                                                mount_path: b
                                                    .common
                                                    .target
                                                    .as_inner()
                                                    .to_string_lossy()
                                                    .to_string(),
                                                ..Default::default()
                                            })
                                        }
                                        compose_spec::service::volumes::Mount::Tmpfs(tmpfs) => {
                                            Ok(k8s_openapi::api::core::v1::VolumeMount {
                                                name: slugify!(
                                                    &tmpfs
                                                        .common
                                                        .target
                                                        .as_inner()
                                                        .to_string_lossy()
                                                ),
                                                mount_path: tmpfs
                                                    .common
                                                    .target
                                                    .as_inner()
                                                    .to_string_lossy()
                                                    .to_string(),
                                                ..Default::default()
                                            })
                                        }
                                        compose_spec::service::volumes::Mount::NamedPipe(_) => {
                                            Err(ComposeServiceError::NamedPipeVolume)
                                        }
                                        compose_spec::service::volumes::Mount::Cluster(_) => {
                                            Err(ComposeServiceError::ClusterVolume)
                                        }
                                    })
                                    .collect::<Result<
                                        Vec<_>,
                                        ComposeServiceError,
                                    >>(
                                    )?;

                                // Add /dev/shm mount if shm_size is specified
                                if self.shm_size.is_some() {
                                    mounts.push(k8s_openapi::api::core::v1::VolumeMount {
                                        name: "dshm".to_string(),
                                        mount_path: "/dev/shm".to_string(),
                                        ..Default::default()
                                    });
                                }

                                // Add tini mount if init is true
                                if self.init {
                                    mounts.push(k8s_openapi::api::core::v1::VolumeMount {
                                        name: "tini".to_string(),
                                        mount_path: "/tini".to_string(),
                                        read_only: Some(true),
                                        ..Default::default()
                                    });
                                }

                                mounts
                            }),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            status: None,
        })
    }
}
