// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::repo::challenges::compose::service::{AsService, HasPorts};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Disk {
    ContainerDisk { image: String },
    CloudInit { cloud_init_user_data_base64: String },
    Pvc { volume_name: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VirtualMachine {
    pub memory: String,
    pub cpu_cores: u32,
    pub disks: Vec<Disk>,
    pub ports: compose_spec::service::ports::Ports,
}

impl HasPorts for VirtualMachine {
    fn get_ports(&self) -> &compose_spec::service::ports::Ports {
        &self.ports
    }
}

impl VirtualMachine {
    pub fn as_kube_virt(&self, id: String) -> k8s_crds_kube_virt::VirtualMachine {
        use k8s_crds_kube_virt::virtualmachines::*;

        VirtualMachine {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(id.clone()),
                labels: Some(
                    [("challengevm".to_string(), id)]
                        .iter()
                        .cloned()
                        .collect(),
                ),
                ..Default::default()
            },
            spec: VirtualMachineSpec {
                running: Some(true),
                template: VirtualMachineTemplate {
                    spec: Some(VirtualMachineTemplateSpec {
                        domain: VirtualMachineTemplateSpecDomain {
                            cpu: Some(VirtualMachineTemplateSpecDomainCpu {
                                cores: Some(self.cpu_cores as i32),
                                ..Default::default()
                            }),
                            resources: Some(VirtualMachineTemplateSpecDomainResources {
                                requests: Some(std::collections::BTreeMap::from([
                                    ("memory".to_string(), k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(self.memory.clone()))
                                ])),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        volumes: Some(self.disks.iter().enumerate().map(|(i, disk)| {
                            match disk {
                                Disk::ContainerDisk { image } => {
                                    VirtualMachineTemplateSpecVolumes {
                                        name: format!("disk-{}", i),
                                        container_disk: Some(VirtualMachineTemplateSpecVolumesContainerDisk {
                                            image: image.clone(),
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    }
                                },
                                Disk::CloudInit { cloud_init_user_data_base64 } => {
                                    VirtualMachineTemplateSpecVolumes {
                                        name: format!("disk-{}", i),
                                        cloud_init_no_cloud: Some(VirtualMachineTemplateSpecVolumesCloudInitNoCloud {
                                            user_data_base64: Some(cloud_init_user_data_base64.clone()),
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    }
                                },
                                Disk::Pvc { volume_name } => {
                                    VirtualMachineTemplateSpecVolumes {
                                        name: format!("disk-{}", i),
                                        persistent_volume_claim: Some(VirtualMachineTemplateSpecVolumesPersistentVolumeClaim {
                                            claim_name: volume_name.clone(),
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    }
                                },
                            }
                        }).collect()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

impl AsService for VirtualMachine {
    fn as_internal_svc(&self, id: String) -> k8s_openapi::api::core::v1::Service {
        k8s_openapi::api::core::v1::Service {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(id.clone()),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::core::v1::ServiceSpec {
                selector: Some(
                    [("challengevm".to_string(), id.clone())]
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