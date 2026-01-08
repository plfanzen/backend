use kube::api::ObjectMeta;

use crate::{
    repo::challenges::compose::service::ComposeServiceError,
    ssh::{SSHGateway, SSHGatewaySpec},
};

impl super::AsSshGateway for compose_spec::Service {
    fn as_ssh_gateways(
        &self,
        id: String,
        ssh_password: Option<String>,
    ) -> Result<Vec<crate::ssh::SSHGateway>, ComposeServiceError> {
        let ssh_ports = compose_spec::service::ports::into_long_iter(self.ports.clone());
        Ok(ssh_ports
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
                        backend_service: id.clone(),
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
