// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use compose_spec::service::ports::Port;
use k8s_crds_traefik::IngressRouteRoutesKind;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::ObjectMeta;

use crate::repo::challenges::compose::service::{ComposeServiceError, HasPortHelpers, HasPorts};

impl<T: HasPorts> super::AsIngress for T {
    fn as_http_ingress(
        &self,
        id: String,
        full_instance_name: &str,
        exposed_domain: &str,
    ) -> Result<Option<k8s_crds_traefik::IngressRoute>, ComposeServiceError> {
        let http_ports = self
            .long_iter_clone()
            .filter(|port| {
                port.app_protocol
                    .as_ref()
                    .is_some_and(|proto| proto.to_uppercase() == "HTTP")
                    && port.protocol.as_ref().is_none_or(|p| p.is_tcp())
            })
            .collect::<Vec<Port>>();
        if http_ports.is_empty() {
            return Ok(None);
        }
        Ok(Some(k8s_crds_traefik::IngressRoute {
            metadata: ObjectMeta {
                name: Some(format!("{}-ingress-route", id)),
                ..Default::default()
            },
            spec: k8s_crds_traefik::ingressroutes::IngressRouteSpec {
                entry_points: Some(vec!["websecure".to_string()]),
                routes: http_ports
                    .iter()
                    .map(|port| k8s_crds_traefik::ingressroutes::IngressRouteRoutes {
                        kind: Some(IngressRouteRoutesKind::Rule),
                        r#match: format!(
                            "Host(`{}-{}-{}.{}`)",
                            id, port.target, full_instance_name, exposed_domain
                        ),
                        services: Some(vec![
                            k8s_crds_traefik::ingressroutes::IngressRouteRoutesServices {
                                name: format!("{}-exposed-ports", id),
                                port: Some(IntOrString::Int(port.target as i32)),
                                ..Default::default()
                            },
                        ]),
                        ..Default::default()
                    })
                    .collect(),
                tls: None,
                parent_refs: None,
            },
        }))
    }

    fn as_tcp_ingress(
        &self,
        id: String,
        full_instance_name: &str,
        exposed_domain: &str,
    ) -> Result<Option<k8s_crds_traefik::IngressRouteTCP>, ComposeServiceError> {
        let external_ports = self
            .long_iter_clone()
            .filter(|port| {
                port.protocol.as_ref().is_none_or(|p| p.is_tcp())
                    && port.app_protocol.as_ref().is_none_or(|app_proto| {
                        app_proto.to_uppercase() != "HTTP" && app_proto.to_uppercase() != "SSH"
                    })
            })
            .collect::<Vec<Port>>();
        if external_ports.is_empty() {
            return Ok(None);
        }
        // Same logic as above, Traefik does TLS termination for TCP as well
        Ok(Some(k8s_crds_traefik::IngressRouteTCP {
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
                                id, port.target, full_instance_name, exposed_domain
                            ),
                            services: Some(vec![
                                k8s_crds_traefik::ingressroutetcps::IngressRouteTCPRoutesServices {
                                    name: format!("{}-exposed-ports", id),
                                    port: IntOrString::Int(port.target as i32),
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
        }))
    }
}
