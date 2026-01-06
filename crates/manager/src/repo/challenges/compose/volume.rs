pub trait AsPvc {
    fn as_pvc(&self, id: String) -> k8s_openapi::api::core::v1::PersistentVolumeClaim;
}

fn get_pvc(name: String, size: String) -> k8s_openapi::api::core::v1::PersistentVolumeClaim {
    k8s_openapi::api::core::v1::PersistentVolumeClaim {
        metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
            name: Some(name),
            ..Default::default()
        },
        spec: Some(k8s_openapi::api::core::v1::PersistentVolumeClaimSpec {
            access_modes: Some(vec!["ReadWriteOnce".to_string()]),
            resources: Some(k8s_openapi::api::core::v1::VolumeResourceRequirements {
                requests: Some(
                    [(
                        "storage".to_string(),
                        k8s_openapi::apimachinery::pkg::api::resource::Quantity(size),
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
impl AsPvc for compose_spec::Volume {
    fn as_pvc(&self, id: String) -> k8s_openapi::api::core::v1::PersistentVolumeClaim {
        get_pvc(
            id,
            self.extensions
                .get("x-size")
                .and_then(|v| v.as_str())
                .unwrap_or("1Gi")
                .to_string(),
        )
    }
}

pub fn default_size_pvc(id: String) -> k8s_openapi::api::core::v1::PersistentVolumeClaim {
    get_pvc(id, "1Gi".to_string())
}
