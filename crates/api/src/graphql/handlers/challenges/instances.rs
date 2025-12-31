// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::{GraphQLEnum, GraphQLObject};

use crate::{db::models::UserRole, graphql::Context, manager_api::Protocol};

#[derive(Debug, Clone, PartialEq, Eq, GraphQLEnum)]
pub enum ConnectionProtocol {
    TcpTls,
    Https,
    Udp,
    Ssh,
    Tcp,
}

#[derive(GraphQLObject, Debug, Clone)]
pub struct CtfChallengeConnectionInfo {
    pub host: String,
    pub port: i32,
    pub protocol: ConnectionProtocol,
}

#[derive(Debug, Clone, PartialEq, Eq, GraphQLEnum)]
pub enum InstanceState {
    Creating,
    Running,
    // Terminating is not reported to users
}

#[derive(GraphQLObject, Debug, Clone)]
pub struct InstanceStatus {
    pub state: InstanceState,
    pub connection_info: Vec<CtfChallengeConnectionInfo>,
}

pub async fn launch_challenge_instance(
    context: &Context,
    challenge_id: String,
) -> juniper::FieldResult<bool> {
    let auth = context.require_authentication()?;

    let mut challenges_client = context.challenges_client();

    challenges_client
        .start_challenge_instance(crate::manager_api::StartChallengeInstanceRequest {
            challenge_id,
            actor: auth.actor(),
            require_release: auth.role == UserRole::Player,
        })
        .await?;

    Ok(true)
}

pub async fn stop_challenge_instance(
    context: &Context,
    challenge_id: String,
) -> juniper::FieldResult<bool> {
    let auth = context.require_authentication()?;

    let mut challenges_client = context.challenges_client();

    challenges_client
        .stop_challenge_instance(crate::manager_api::StopChallengeInstanceRequest {
            challenge_id,
            actor: auth.actor(),
        })
        .await?;

    Ok(true)
}

pub async fn get_challenge_instance_status(
    context: &Context,
    challenge_id: String,
) -> juniper::FieldResult<Option<InstanceStatus>> {
    let auth = context.require_authentication()?;

    let mut challenges_client = context.challenges_client();

    let response = challenges_client
        .get_challenge_instance_status(crate::manager_api::GetChallengeInstanceStatusRequest {
            challenge_id,
            actor: auth.actor(),
        })
        .await?
        .into_inner();

    if response.is_deployed == false {
        return Ok(None);
    }

    Ok(Some(InstanceStatus {
        state: if response.is_ready {
            InstanceState::Running
        } else {
            InstanceState::Creating
        },
        connection_info: response
            .connection_info
            .into_iter()
            .filter_map(|ci| {
                Some(CtfChallengeConnectionInfo {
                    host: ci.host,
                    port: ci.port as i32,
                    protocol: match Protocol::try_from(ci.protocol).ok()? {
                        Protocol::TcpTls => ConnectionProtocol::TcpTls,
                        Protocol::Https => ConnectionProtocol::Https,
                        Protocol::Udp => ConnectionProtocol::Udp,
                        Protocol::Ssh => ConnectionProtocol::Ssh,
                        Protocol::Tcp => ConnectionProtocol::Tcp,
                    },
                })
            })
            .collect(),
    }))
}
