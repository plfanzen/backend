use kube::api::ObjectMeta;

use crate::{
    repo::challenges::compose::service::{ComposeServiceError, HasPortHelpers, HasPorts},
    ssh::{SSHGateway, SSHGatewaySpec},
};

impl<T: HasPorts> super::AsSshGateway for T {
    fn as_ssh_gateways(
        &self,
        id: String,
        ssh_password: Option<String>,
    ) -> Result<Vec<crate::ssh::SSHGateway>, ComposeServiceError> {
        Ok(self
            .long_iter_clone()
            .filter_map(|port| {
                let is_ssh = port
                    .app_protocol
                    .as_ref()
                    .is_some_and(|proto| proto.to_lowercase() == "ssh")
                    && port.protocol.as_ref().is_none_or(|p| p.is_tcp());

                if !is_ssh {
                    return None;
                };

                let Some(username) = port
                    .extensions
                    .get("x-username")
                    .and_then(|u| u.as_str().map(|str| str.to_string()))
                else {
                    tracing::warn!(
                        "SSH port does not declare x-username as string: {:#?}",
                        port
                    );
                    return None;
                };
                let Some(password) = port
                    .extensions
                    .get("x-password")
                    .and_then(|u| u.as_str().map(|str| str.to_string()))
                else {
                    tracing::warn!(
                        "SSH port does not declare x-password as string: {:#?}",
                        port
                    );
                    return None;
                };
                Some(SSHGateway {
                    metadata: ObjectMeta {
                        name: Some(format!(
                            "{}-{}",
                            id,
                            port.published.map(|r| r.start()).unwrap_or(port.target)
                        )),
                        ..Default::default()
                    },
                    spec: SSHGatewaySpec {
                        backend_service: format!("{}-exposed-ports", id),
                        backend_port: port.target,
                        backend_username: username,
                        backend_password: password,
                        gateway_password: ssh_password.clone(),
                    },
                })
            })
            .collect())
    }
}
