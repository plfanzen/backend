// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use kube::api::ObjectMeta;

use crate::repo::challenges::compose::service::ComposeServiceError;

impl super::AsService for compose_spec::Service {
    fn as_internal_svc(&self, id: String) -> k8s_openapi::api::core::v1::Service {
        k8s_openapi::api::core::v1::Service {
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
                cluster_ip: Some("None".to_string()),
                ports: None,
                ..Default::default()
            }),
            status: None,
        }
    }
}

impl super::AsExternalService for compose_spec::Service {
    // This is still not publicly exposed, but will be targeted by Traefik
    // We currently do not use LoadBalancer services, but rather have this being proxied by Traefik
    // In the future, we may want to support LoadBalancer services
    fn as_proxied_svc(
        &self,
        id: String,
    ) -> Result<Option<k8s_openapi::api::core::v1::Service>, ComposeServiceError> {
        if self.ports.is_empty() {
            return Ok(None);
        }
        Ok(Some(k8s_openapi::api::core::v1::Service {
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
                    compose_spec::service::ports::into_long_iter(self.ports.clone())
                        .map(|port| {
                            if port.host_ip.is_some() {
                                return Err(ComposeServiceError::PortWithHostIp);
                            }
                            Ok(k8s_openapi::api::core::v1::ServicePort {
                                name: port.name,
                                // For now, we use a simple implementation of ranges by only taking the start of the published port range
                                port: port
                                    .published
                                    .map(|r| r.start())
                                    .unwrap_or(port.target as u16)
                                    as i32,
                                target_port: Some(
                                    k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(
                                        port.target as i32,
                                    ),
                                ),
                                protocol: Some(match port.protocol {
                                    Some(compose_spec::service::ports::Protocol::Tcp) | None => {
                                        "TCP".to_string()
                                    }
                                    // we don't support UDP at the moment, because it would require loadbalancer stuff, and I don't want to deal with that now
                                    Some(compose_spec::service::ports::Protocol::Udp)
                                    | Some(compose_spec::service::ports::Protocol::Other(_)) => {
                                        return Err(ComposeServiceError::Other(
                                            "Unsupported protocol in port definition".to_string(),
                                        ));
                                    }
                                }),
                                ..Default::default()
                            })
                        })
                        .collect::<Result<Vec<_>, ComposeServiceError>>()?,
                ),
                ..Default::default()
            }),
            status: None,
        }))
    }

    fn as_lb_svc(
        &self,
        _id: String,
    ) -> Result<Option<k8s_openapi::api::core::v1::Service>, ComposeServiceError> {
        Ok(None)
    }
}
