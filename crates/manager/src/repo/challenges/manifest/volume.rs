use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChallengeVolume {
    pub size: String,
}

impl ChallengeVolume {
    pub fn get_pvc(&self, id: String) -> k8s_openapi::api::core::v1::PersistentVolumeClaim {
        k8s_openapi::api::core::v1::PersistentVolumeClaim {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(id),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::core::v1::PersistentVolumeClaimSpec {
                access_modes: Some(vec!["ReadWriteOnce".to_string()]),
                resources: Some(k8s_openapi::api::core::v1::VolumeResourceRequirements {
                    requests: Some(
                        [(
                            "storage".to_string(),
                            k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                self.size.clone(),
                            ),
                        )]
                        .iter()
                        .cloned()
                        .collect(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
