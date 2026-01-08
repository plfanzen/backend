// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use slugify::slugify;

use crate::repo::challenges::compose::service::ComposeServiceError;

/// Builds Kubernetes volumes from compose service configuration
pub fn build_volumes(
    svc: &compose_spec::Service,
) -> Result<Vec<k8s_openapi::api::core::v1::Volume>, ComposeServiceError> {
    let mut volumes: Vec<k8s_openapi::api::core::v1::Volume> =
        compose_spec::service::volumes::into_long_iter(svc.volumes.clone())
            .map(convert_volume)
            .collect::<Result<Vec<_>, ComposeServiceError>>()?;

    // Add /dev/shm volume if shm_size is specified
    if let Some(shm_size) = &svc.shm_size {
        volumes.push(k8s_openapi::api::core::v1::Volume {
            name: "dshm".to_string(),
            empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                medium: Some("Memory".to_string()),
                size_limit: Some(k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                    shm_size.to_string(),
                )),
            }),
            ..Default::default()
        });
    }

    // Add tmpfs volumes
    if let Some(tmpfs_mounts) = &svc.tmpfs {
        for item in tmpfs_mounts.clone().into_list() {
            let mount_path = item.as_inner();
            volumes.push(k8s_openapi::api::core::v1::Volume {
                name: slugify!(&mount_path.to_string_lossy()),
                empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                    medium: Some("Memory".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
    }

    // Add tini volume if init is true
    if svc.init {
        volumes.push(k8s_openapi::api::core::v1::Volume {
            name: "tini".to_string(),
            empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource::default()),
            ..Default::default()
        });
    }

    Ok(volumes)
}

fn convert_volume(
    vol: compose_spec::service::volumes::Mount,
) -> Result<k8s_openapi::api::core::v1::Volume, ComposeServiceError> {
    match vol {
        compose_spec::service::volumes::Mount::Volume(volume) => {
            let vol_name = volume
                .source
                .as_ref()
                .ok_or(ComposeServiceError::AnonymousVolume)?
                .clone();
            Ok(k8s_openapi::api::core::v1::Volume {
                name: vol_name.to_string(),
                persistent_volume_claim: Some(
                    k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                        claim_name: vol_name.to_string(),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            })
        }
        compose_spec::service::volumes::Mount::Bind(b) => {
            let host_path = b.source.as_inner();
            if !host_path.starts_with("./data/") {
                return Err(ComposeServiceError::HostPathVolume(
                    host_path.to_string_lossy().to_string(),
                ));
            }
            Ok(k8s_openapi::api::core::v1::Volume {
                name: slugify!(&b.common.target.as_inner().to_string_lossy()),
                persistent_volume_claim: Some(
                    k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                        claim_name: "plfanzen_internal_ctf_data".to_string(),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            })
        }
        compose_spec::service::volumes::Mount::Tmpfs(tmpfs) => {
            Ok(k8s_openapi::api::core::v1::Volume {
                name: slugify!(&tmpfs.common.target.as_inner().to_string_lossy()),
                empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                    medium: Some("Memory".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            })
        }
        compose_spec::service::volumes::Mount::NamedPipe(_) => {
            Err(ComposeServiceError::NamedPipeVolume)
        }
        compose_spec::service::volumes::Mount::Cluster(_) => {
            Err(ComposeServiceError::ClusterVolume)
        }
    }
}

/// Builds volume mounts for the container
pub fn build_volume_mounts(
    svc: &compose_spec::Service,
) -> Result<Vec<k8s_openapi::api::core::v1::VolumeMount>, ComposeServiceError> {
    let mut mounts: Vec<k8s_openapi::api::core::v1::VolumeMount> =
        compose_spec::service::volumes::into_long_iter(svc.volumes.clone())
            .map(convert_volume_mount)
            .collect::<Result<Vec<_>, ComposeServiceError>>()?;

    // Add /dev/shm mount if shm_size is specified
    if svc.shm_size.is_some() {
        mounts.push(k8s_openapi::api::core::v1::VolumeMount {
            name: "dshm".to_string(),
            mount_path: "/dev/shm".to_string(),
            ..Default::default()
        });
    }

    // Add tini mount if init is true
    if svc.init {
        mounts.push(k8s_openapi::api::core::v1::VolumeMount {
            name: "tini".to_string(),
            mount_path: "/tini".to_string(),
            read_only: Some(true),
            ..Default::default()
        });
    }

    Ok(mounts)
}

fn convert_volume_mount(
    vol: compose_spec::service::volumes::Mount,
) -> Result<k8s_openapi::api::core::v1::VolumeMount, ComposeServiceError> {
    match vol {
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
                name: slugify!(&b.common.target.as_inner().to_string_lossy()),
                mount_path: b.common.target.as_inner().to_string_lossy().to_string(),
                ..Default::default()
            })
        }
        compose_spec::service::volumes::Mount::Tmpfs(tmpfs) => {
            Ok(k8s_openapi::api::core::v1::VolumeMount {
                name: slugify!(&tmpfs.common.target.as_inner().to_string_lossy()),
                mount_path: tmpfs.common.target.as_inner().to_string_lossy().to_string(),
                ..Default::default()
            })
        }
        compose_spec::service::volumes::Mount::NamedPipe(_) => {
            Err(ComposeServiceError::NamedPipeVolume)
        }
        compose_spec::service::volumes::Mount::Cluster(_) => {
            Err(ComposeServiceError::ClusterVolume)
        }
    }
}
