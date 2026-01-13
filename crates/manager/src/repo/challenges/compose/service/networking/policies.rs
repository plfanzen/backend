use k8s_crds_cilium::ciliumnetworkpolicies::*;
use std::collections::BTreeMap;

use super::{OtherParty, Protocol};

impl super::NetworkPolicy {
    pub fn as_networkpolicy(
        &self,
        policy_name: &str,
        match_labels: Option<BTreeMap<String, String>>,
    ) -> CiliumNetworkPolicy {
        let spec = CiliumNetworkPolicySpec {
            description: Some("Base challenge policy".to_string()),
            endpoint_selector: Some(CiliumNetworkPolicyEndpointSelector {
                match_labels,
                match_expressions: None,
            }),
            ingress: Some(
                self.incoming
                    .rules
                    .iter()
                    .map(|rule| CiliumNetworkPolicyIngress {
                        from_entities: match rule.other_party {
                            OtherParty::Cluster => Some(vec!["cluster".to_string()]),
                            OtherParty::World => Some(vec!["world".to_string()]),
                            _ => None,
                        },
                        to_ports: rule.ports.as_ref().map(|ports| {
                            ports
                                .iter()
                                .map(|port_rule| CiliumNetworkPolicyIngressToPorts {
                                    ports: Some(port_rule.protocols.iter().map(|protocol| {
                                        CiliumNetworkPolicyIngressToPortsPorts {
                                            port: port_rule.port.to_string(),
                                            end_port: None,
                                            protocol: match protocol {
                                                Protocol::TCP => Some(
                                                    CiliumNetworkPolicyIngressToPortsPortsProtocol::Tcp,
                                                ),
                                                Protocol::UDP => Some(
                                                    CiliumNetworkPolicyIngressToPortsPortsProtocol::Udp,
                                                ),
                                            },
                                        }
                                    }).collect()),
                                    ..Default::default()
                                })
                                .collect()
                        }),
                        ..Default::default()
                    })
                    .collect(),
            ),
            egress: Some(
                self.outgoing
                    .rules
                    .iter()
                    .map(|rule| {
                        if rule.other_party == OtherParty::ClusterDns {
                            return CiliumNetworkPolicyEgress {
                                to_endpoints: Some(vec![CiliumNetworkPolicyEgressToEndpoints {
                                    match_labels: Some(BTreeMap::from([
                                        (
                                            "io.kubernetes.pod.namespace".to_string(),
                                            "kube-system".to_string(),
                                        ),
                                        ("k8s-app".to_string(), "kube-dns".to_string()),
                                    ])),
                                    ..Default::default()
                                }]),
                                to_ports: Some(vec![CiliumNetworkPolicyEgressToPorts {
                                    ports: Some(vec![
                                        CiliumNetworkPolicyEgressToPortsPorts {
                                            port: "53".to_string(),
                                            protocol: Some(
                                                CiliumNetworkPolicyEgressToPortsPortsProtocol::Udp,
                                            ),
                                            end_port: None,
                                        },
                                        CiliumNetworkPolicyEgressToPortsPorts {
                                            port: "53".to_string(),
                                            protocol: Some(
                                                CiliumNetworkPolicyEgressToPortsPortsProtocol::Tcp,
                                            ),
                                            end_port: None,
                                        },
                                    ]),
                                    rules: if std::env::var("INSECURE_FORCE_DISABLE_DNS_CHECKS")
                                        .is_err()
                                    {
                                        Some(CiliumNetworkPolicyEgressToPortsRules {
                                            dns: Some(vec![CiliumNetworkPolicyEgressToPortsRulesDns {
                                                match_pattern: Some("*".to_string()),
                                                ..Default::default()
                                            }]),
                                            ..Default::default()
                                        })
                                    } else {
                                        None
                                    },
                                    ..Default::default()
                                }]),
                                ..Default::default()
                            };
                        };
                        CiliumNetworkPolicyEgress {
                            to_endpoints: Some(vec![CiliumNetworkPolicyEgressToEndpoints {
                                match_labels: Some({
                                    let mut labels = BTreeMap::new();
                                    match rule.other_party {
                                        OtherParty::Challenge => {
                                            labels
                                                .insert("app".to_string(), "challenge".to_string());
                                        }
                                        OtherParty::Cluster => {
                                            labels.insert(
                                                "k8s-app".to_string(),
                                                "kubelet".to_string(),
                                            );
                                        }
                                        OtherParty::World => {
                                            labels.insert("world".to_string(), "true".to_string());
                                        }
                                        OtherParty::ClusterDns => {}
                                    }
                                    labels
                                }),
                                match_expressions: None,
                            }]),
                            to_ports: rule.ports.as_ref().map(|ports| {
                                ports
                                    .iter()
                                    .map(|port_rule| CiliumNetworkPolicyEgressToPorts {
                                        ports: Some(port_rule.protocols.iter().map(|protocol| {
                                            CiliumNetworkPolicyEgressToPortsPorts {
                                                port: port_rule.port.to_string(),
                                                end_port: None,
                                                protocol: match protocol {
                                                    Protocol::TCP => Some(
                                                        CiliumNetworkPolicyEgressToPortsPortsProtocol::Tcp,
                                                    ),
                                                    Protocol::UDP => Some(
                                                        CiliumNetworkPolicyEgressToPortsPortsProtocol::Udp,
                                                    ),
                                                },
                                            }
                                        }).collect()),
                                        ..Default::default()
                                    })
                                    .collect()
                            }),
                            ..Default::default()
                        }
                    })
                    .collect(),
            ),
            ..Default::default()
        };

        CiliumNetworkPolicy {
            metadata: kube::api::ObjectMeta {
                name: Some(policy_name.to_string()),
                ..Default::default()
            },
            spec,
            status: None,
        }
    }
}
