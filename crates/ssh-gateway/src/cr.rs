use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "SSHGateway",
    group = "plfanzen.garden",
    version = "v1alpha1",
    namespaced
)]
pub struct SSHGatewaySpec {
    pub backend_service: String,
    pub backend_port: u16,
    pub backend_username: String,
    pub backend_password: String,
    /// The password the user will use to login to the SSH gateway (if empty, accept any password)
    pub gateway_password: Option<String>,
}
