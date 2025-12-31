use std::collections::HashMap;

use k8s_crds_traefik::IngressRouteRoutesKind;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::ObjectMeta;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Protocol {
    HTTP,
    TCP,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExposedPort {
    pub port: u16,
    pub protocol: Protocol,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChallengeService {
    pub image: String,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub mount_volumes: HashMap<String, String>,
    #[serde(default)]
    pub tmp_dirs: HashMap<String, String>,
    #[serde(default)]
    pub read_only: bool,
    pub internal_ports: Option<HashMap<u16, u16>>,
    #[serde(default)]
    pub external_ports: Vec<ExposedPort>,
}

impl ChallengeService {
    pub fn get_deployment(&self, id: String) -> k8s_openapi::api::apps::v1::Deployment {
        k8s_openapi::api::apps::v1::Deployment {
            metadata: ObjectMeta {
                name: Some(id.clone()),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::apps::v1::DeploymentSpec {
                replicas: Some(1),
                selector: k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector {
                    match_labels: Some([("component".to_string(), id.clone())].iter().cloned().collect()),
                    ..Default::default()
                },
                template: k8s_openapi::api::core::v1::PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: Some([("component".to_string(), id.clone())].iter().cloned().collect()),
                        ..Default::default()
                    }),
                    spec: Some(k8s_openapi::api::core::v1::PodSpec {
                        volumes: Some(
                            self.tmp_dirs
                                .iter()
                                .map(|(vol_name, _)| k8s_openapi::api::core::v1::Volume {
                                    name: vol_name.clone(),
                                    empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                })
                                .chain(
                                    self.mount_volumes.iter().map(|(vol_name, _)| {
                                        k8s_openapi::api::core::v1::Volume {
                                            name: vol_name.clone(),
                                            persistent_volume_claim: Some(
                                                k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                                                    claim_name: vol_name.clone(),
                                                    ..Default::default()
                                                },
                                            ),
                                            ..Default::default()
                                        }
                                    }),
                                )
                                .collect(),
                        ),
                        containers: vec![k8s_openapi::api::core::v1::Container {
                            name: id,
                            image: Some(self.image.clone()),
                            env: Some(
                                self.environment
                                    .iter()
                                    .map(|(k, v)| k8s_openapi::api::core::v1::EnvVar {
                                        name: k.clone(),
                                        value: Some(v.clone()),
                                        ..Default::default()
                                    })
                                    .collect(),
                            ),
                            volume_mounts: Some(
                                self.mount_volumes
                                    .iter()
                                    .map(|(vol_name, mount_path)| k8s_openapi::api::core::v1::VolumeMount {
                                        name: vol_name.clone(),
                                        mount_path: mount_path.clone(),
                                        read_only: Some(self.read_only),
                                        ..Default::default()
                                    })
                                    .chain(
                                        self.tmp_dirs.iter().map(|(vol_name, mount_path)| {
                                            k8s_openapi::api::core::v1::VolumeMount {
                                                name: vol_name.clone(),
                                                mount_path: mount_path.clone(),
                                                read_only: Some(self.read_only),
                                                ..Default::default()
                                            }
                                        }),
                                    )
                                    .collect(),
                            ),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            status: None,
        }
    }

    pub fn get_internal_svc(&self, id: String) -> Option<k8s_openapi::api::core::v1::Service> {
        if self.internal_ports.as_ref().is_some_and(|p| p.is_empty()) {
            return None;
        }
        Some(k8s_openapi::api::core::v1::Service {
            metadata: ObjectMeta {
                name: Some(id.clone()),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::core::v1::ServiceSpec {
                selector: Some(
                    [("component".to_string(), id.clone())]
                        .iter()
                        .cloned()
                        .collect(),
                ),
                cluster_ip: if self.internal_ports.is_none() {
                    Some("None".to_string())
                } else {
                    None
                },
                ports: self.internal_ports.as_ref().map(|ports| {
                    ports
                        .iter()
                        .map(
                            |(internal, external)| k8s_openapi::api::core::v1::ServicePort {
                                port: *external as i32,
                                target_port: Some(
                                    k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(
                                        *internal as i32,
                                    ),
                                ),
                                ..Default::default()
                            },
                        )
                        .collect()
                }),

                ..Default::default()
            }),
            status: None,
        })
    }

    // This is still not publicly exposed, but will be targeted by Traefik
    pub fn get_external_svc(&self, id: String) -> Option<k8s_openapi::api::core::v1::Service> {
        if self.external_ports.is_empty() {
            return None;
        }
        Some(k8s_openapi::api::core::v1::Service {
            metadata: ObjectMeta {
                name: Some(format!("{}-exposed-ports", id.clone())),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::core::v1::ServiceSpec {
                selector: Some(
                    [("component".to_string(), id.clone())]
                        .iter()
                        .cloned()
                        .collect(),
                ),
                ports: Some(
                    self.external_ports
                        .iter()
                        .map(|port| k8s_openapi::api::core::v1::ServicePort {
                            port: port.port as i32,
                            target_port: Some(
                                k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(
                                    port.port as i32,
                                ),
                            ),
                            protocol: Some(match port.protocol {
                                Protocol::HTTP => "TCP".to_string(),
                                Protocol::TCP => "TCP".to_string(),
                            }),
                            ..Default::default()
                        })
                        .collect(),
                ),
                ..Default::default()
            }),
            status: None,
        })
    }

    pub fn get_ingress_route(
        &self,
        id: String,
        full_instance_name: &str,
        exposed_domain: &str,
    ) -> Option<k8s_crds_traefik::IngressRoute> {
        let external_ports = self
            .external_ports
            .iter()
            .filter(|port| matches!(port.protocol, Protocol::HTTP))
            .collect::<Vec<&ExposedPort>>();
        if external_ports.is_empty() {
            return None;
        }
        Some(k8s_crds_traefik::IngressRoute {
            metadata: ObjectMeta {
                name: Some(format!("{}-ingress-route", id)),
                ..Default::default()
            },
            spec: k8s_crds_traefik::ingressroutes::IngressRouteSpec {
                entry_points: Some(vec!["websecure".to_string()]),
                routes: external_ports
                    .iter()
                    .map(|port| k8s_crds_traefik::ingressroutes::IngressRouteRoutes {
                        kind: Some(IngressRouteRoutesKind::Rule),
                        r#match: format!(
                            "Host(`{}`)",
                            format!(
                                "{}-{}-{}.{}",
                                id, port.port, full_instance_name, exposed_domain
                            )
                        ),
                        services: Some(vec![
                            k8s_crds_traefik::ingressroutes::IngressRouteRoutesServices {
                                name: format!("{}-exposed-ports", id),
                                port: Some(IntOrString::Int(port.port as i32)),
                                ..Default::default()
                            },
                        ]),
                        ..Default::default()
                    })
                    .collect(),
                tls: None,
                parent_refs: None,
            },
        })
    }

    pub fn get_ingress_route_tcp(
        &self,
        id: String,
        full_instance_name: &str,
        exposed_domain: &str,
    ) -> Option<k8s_crds_traefik::IngressRouteTCP> {
        let external_ports = self
            .external_ports
            .iter()
            .filter(|port| matches!(port.protocol, Protocol::TCP))
            .collect::<Vec<&ExposedPort>>();
        if external_ports.is_empty() {
            return None;
        }
        // Same logic as above, Traefik does TLS termination for TCP as well
        Some(k8s_crds_traefik::IngressRouteTCP {
            metadata: ObjectMeta {
                name: Some(format!("{}-ingress-route-tcp", id)),
                ..Default::default()
            },
            spec: k8s_crds_traefik::ingressroutetcps::IngressRouteTCPSpec {
                entry_points: Some(vec!["websecure".to_string()]),
                routes: external_ports
                    .iter()
                    .map(
                        |port| k8s_crds_traefik::ingressroutetcps::IngressRouteTCPRoutes {
                            r#match: format!(
                                "HostSNI(`{}-{}-{}.{}`)",
                                id, port.port, full_instance_name, exposed_domain
                            ),
                            services: Some(vec![
                                k8s_crds_traefik::ingressroutetcps::IngressRouteTCPRoutesServices {
                                    name: format!("{}-exposed-ports", id),
                                    port: IntOrString::Int(port.port as i32),
                                    ..Default::default()
                                },
                            ]),
                            ..Default::default()
                        },
                    )
                    .collect(),
                tls: Some(k8s_crds_traefik::ingressroutetcps::IngressRouteTCPTls {
                    passthrough: Some(false),
                    ..Default::default()
                }),
            },
        })
    }
}
