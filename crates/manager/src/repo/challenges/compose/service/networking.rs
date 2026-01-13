mod policies;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::repo::challenges::{loader::Challenge, vm::{HasVms, VirtualMachine}};

#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum OtherParty {
    Challenge,
    Cluster,
    ClusterDns,
    #[default]
    World,
}

#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Protocol {
    #[default]
    TCP,
    UDP,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortRule {
    pub port: u16,
    pub protocols: Vec<Protocol>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetworkPolicyRule {
    #[serde(default)]
    pub other_party: OtherParty,
    pub ports: Option<Vec<PortRule>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IncomingNetworkPolicy {
    pub rules: Vec<NetworkPolicyRule>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutgoingNetworkPolicy {
    pub rules: Vec<NetworkPolicyRule>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetworkPolicy {
    pub incoming: IncomingNetworkPolicy,
    pub outgoing: OutgoingNetworkPolicy,
}

impl Default for IncomingNetworkPolicy {
    fn default() -> Self {
        IncomingNetworkPolicy {
            rules: vec![
                NetworkPolicyRule {
                    other_party: OtherParty::Cluster,
                    ports: None,
                },
                NetworkPolicyRule {
                    other_party: OtherParty::World,
                    ports: None,
                },
            ],
        }
    }
}

impl Default for OutgoingNetworkPolicy {
    fn default() -> Self {
        OutgoingNetworkPolicy {
            rules: vec![
                NetworkPolicyRule {
                    other_party: OtherParty::Challenge,
                    ports: None,
                },
                NetworkPolicyRule {
                    other_party: OtherParty::World,
                    ports: None,
                },
                NetworkPolicyRule {
                    other_party: OtherParty::ClusterDns,
                    ports: Some(vec![PortRule {
                        port: 53,
                        protocols: vec![Protocol::UDP, Protocol::TCP],
                    }]),
                },
            ],
        }
    }
}

pub trait HasNetworkPolicy {
    fn get_network_policy(&self) -> Option<NetworkPolicy>;
}

impl HasNetworkPolicy for compose_spec::service::Service {
    fn get_network_policy(&self) -> Option<NetworkPolicy> {
        self.extensions.get("x-ctf-network-policy").and_then(|v| {
            match serde_yaml::from_value::<NetworkPolicy>(v.clone()) {
                Ok(policy) => Some(policy),
                Err(err) => {
                    tracing::error!(
                        "Failed to parse x-ctf-network-policy for service: {}",
                        err
                    );
                    None
                }
            }
        })
    }
}

impl HasNetworkPolicy for VirtualMachine {
    fn get_network_policy(&self) -> Option<NetworkPolicy> {
        self.network_policy.clone()
    }
}

pub fn get_policies(challenge: &Challenge) -> Vec<k8s_crds_cilium::CiliumNetworkPolicy> {
    let base_policy = match challenge
        .compose
        .extensions
        .get("x-ctf-network-policy")
        .and_then(|v| serde_yaml::from_value::<NetworkPolicy>(v.clone()).ok())
    {
        Some(policy) => policy,
        None => NetworkPolicy {
            incoming: IncomingNetworkPolicy::default(),
            outgoing: OutgoingNetworkPolicy::default(),
        },
    };

    let mut policies = vec![];

    policies.push(base_policy.as_networkpolicy(
        "base",
        Some(BTreeMap::from([(
            "networkpolicy".to_string(),
            "base".to_string(),
        )])),
    ));

    for (svc_id, svc) in &challenge.compose.services {
        let Some(policy) = svc.get_network_policy() else {
            continue;
        };
        policies.push(policy.as_networkpolicy(
            format!("svc-{}", svc_id).as_str(),
            Some(BTreeMap::from([(
                "compose-service-id".to_string(),
                svc_id.to_string(),
            )])),
        ));
    }

    for (vm_id, vm) in challenge.compose.get_vms() {
        let Some(policy) = vm.get_network_policy() else {
            continue;
        };
        policies.push(policy.as_networkpolicy(
            format!("vm-{}", vm_id).as_str(),
            Some(BTreeMap::from([(
                "virtual-machine-id".to_string(),
                vm_id.to_string(),
            )])),
        ));
    }

    policies
}
