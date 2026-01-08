// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::path::PathBuf;

use tonic::Response;

use crate::grpc::api::{
    Challenge, CheckFlagRequest, CheckFlagResponse, ConnectionInfo, ExportChallengeRequest,
    ExportChallengeResponse, GetChallengeInstanceStatusRequest, GetChallengeInstanceStatusResponse,
    ListChallengesRequest, ListChallengesResponse, Protocol, RetrieveFileRequest,
    RetrieveFileResponse, StartChallengeInstanceRequest, StartChallengeInstanceResponse,
    StopChallengeInstanceRequest, StopChallengeInstanceResponse,
};
use crate::instances::InstanceState;
use crate::repo::challenges::loader::tera::render_dir_recursively;
use crate::repo::challenges::loader::{load_challenge_from_repo, load_challenges_from_repo};

use super::api::challenges_service_server::ChallengesService;
pub struct ChallengeManager {
    pub repo_dir: PathBuf,
    pub kube_client: kube::Client,
}

fn get_connection_details(
    challenge: &compose_spec::Compose,
    challenge_id: &str,
    instance_id: &str,
) -> Vec<ConnectionInfo> {
    let mut connection_info = vec![];
    for (svc_id, svc) in &challenge.services {
        for exposed_port in compose_spec::service::ports::into_long_iter(svc.ports.clone()) {
            let uses_ssh_gateway = exposed_port
                .app_protocol
                .as_ref()
                .is_some_and(|p| p.to_lowercase() == "ssh")
                && exposed_port.extensions.contains_key("x-username")
                && exposed_port.extensions.contains_key("x-password");
            connection_info.push(ConnectionInfo {
                host: format!(
                    "{}-{}-challenge-{}-instance-{}.{}",
                    svc_id,
                    exposed_port
                        .published
                        .map(|r| r.start())
                        .unwrap_or(exposed_port.target),
                    challenge_id,
                    instance_id,
                    std::env::var("EXPOSED_DOMAIN").unwrap_or("localhost".to_string())
                ),
                port: 443,
                protocol: if exposed_port.protocol.as_ref().is_none_or(|p| p.is_tcp()) {
                    match exposed_port.app_protocol {
                        Some(proto) if proto.to_lowercase() == "http".to_string() => {
                            Protocol::Https as i32
                        }
                        Some(proto) if proto.to_lowercase() == "ssh".to_string() => {
                            Protocol::Ssh as i32
                        }
                        _ => Protocol::Tcp as i32,
                    }
                } else if exposed_port.protocol.as_ref().is_some_and(|p| p.is_udp()) {
                    Protocol::Udp as i32
                } else {
                    continue;
                },
                ssh_username: if uses_ssh_gateway {
                    Some(format!(
                        "{}-{}-challenge-{}-instance-{}",
                        svc_id,
                        exposed_port
                            .published
                            .map(|r| r.start())
                            .unwrap_or(exposed_port.target),
                        challenge_id,
                        instance_id,
                    ))
                } else {
                    None
                },
                ssh_password: None,
            });
        }
    }
    connection_info
}

#[tonic::async_trait]
impl ChallengesService for ChallengeManager {
    /// ListChallenges returns a list of all available challenges.
    async fn list_challenges(
        &self,
        request: tonic::Request<ListChallengesRequest>,
    ) -> Result<tonic::Response<ListChallengesResponse>, tonic::Status> {
        let request = request.into_inner();
        let challenges = load_challenges_from_repo(&self.repo_dir, &request.actor, false)
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to load challenges: {}", e)))?;

        let event_config = crate::repo::EventConfig::try_load_from_repo(&self.repo_dir)
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to load event config: {}", e)))?;
        let mut out_challenges = vec![];
        for (id, chall) in challenges {
            if request.require_release {
                let now = chrono::Utc::now().timestamp() as u64;
                if let Some(release_time) = chall.metadata.release_time {
                    if now < release_time {
                        continue;
                    }
                }
            }
            let solve_info = request.solved_challenges.get(&id);
            let points = event_config
                .calculate_points(
                    &chall.metadata,
                    solve_info
                        .as_ref()
                        .map(|s| s.current_solves as u32)
                        .unwrap_or(0),
                    solve_info
                        .as_ref()
                        .map(|s| s.actor_nth_solve as u32)
                        .unwrap_or(0),
                    request.total_competitors as u32,
                )
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to calculate points for challenge {}: {}",
                        id, e
                    ))
                })?;
            out_challenges.push(Challenge {
                id: id,
                name: chall.metadata.name,
                description: chall.metadata.description_md,
                release_timestamp: chall.metadata.release_time,
                end_timestamp: chall.metadata.end_time,
                categories: chall.metadata.categories,
                authors: chall.metadata.authors,
                attachments: chall.metadata.attachments,
                can_start: !chall.compose.services.is_empty(),
                points,
                difficulty: chall.metadata.difficulty,
                can_export: chall.metadata.auto_publish_src,
            });
        }
        let response = ListChallengesResponse {
            challenges: out_challenges,
        };
        Ok(tonic::Response::new(response))
    }

    /// StartChallengeInstance starts a new instance of the specified challenge for the given team.
    async fn start_challenge_instance(
        &self,
        request: tonic::Request<StartChallengeInstanceRequest>,
    ) -> Result<tonic::Response<StartChallengeInstanceResponse>, tonic::Status> {
        let request = request.into_inner();
        if !request
            .challenge_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(tonic::Status::invalid_argument(
                "challenge_id contains invalid characters",
            ));
        }
        let challenge =
            load_challenge_from_repo(&self.repo_dir, &request.challenge_id, &request.actor, false)
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to load challenge {} from repo: {}",
                        request.challenge_id, e
                    ))
                })?;

        if challenge.compose.services.is_empty() {
            return Err(tonic::Status::failed_precondition(format!(
                "Challenge {} has no services to start",
                request.challenge_id
            )));
        }

        if request.require_release {
            let now = chrono::Utc::now().timestamp() as u64;
            if let Some(release_time) = challenge.metadata.release_time {
                if now < release_time {
                    return Err(tonic::Status::failed_precondition(format!(
                        "Challenge {} has not been released yet",
                        request.challenge_id
                    )));
                }
            }
        }

        let instance_id = crate::instances::prepare_instance(
            &self.kube_client,
            &request.challenge_id,
            &request.actor,
        )
        .await
        .map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to start challenge instance for challenge {}: {}",
                request.challenge_id, e
            ))
        })?;
        let connection_info =
            get_connection_details(&challenge.compose, &request.challenge_id, &instance_id);

        let working_dir = tempfile::tempdir().map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to create temporary working directory: {}",
                e
            ))
        })?;

        render_dir_recursively(
            &self.repo_dir.join("challs").join(&request.challenge_id),
            &working_dir.path(),
            &request.actor,
            false,
        )
        .map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to render challenge templates for challenge {}: {}",
                request.challenge_id, e
            ))
        })?;

        crate::instances::deploy::deploy_challenge(
            &self.kube_client,
            &instance_id,
            challenge,
            &std::env::var("EXPOSED_DOMAIN").unwrap_or("localhost".to_string()),
            &working_dir.path(),
        )
        .await
        .map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to deploy challenge instance for challenge {}: {}",
                request.challenge_id, e
            ))
        })?;
        let response = StartChallengeInstanceResponse {
            instance_id: instance_id,
            connection_info,
        };
        Ok(Response::new(response))
    }

    /// StopChallengeInstance stops the specified challenge instance for the given team.
    async fn stop_challenge_instance(
        &self,
        request: tonic::Request<StopChallengeInstanceRequest>,
    ) -> Result<tonic::Response<StopChallengeInstanceResponse>, tonic::Status> {
        let request = request.into_inner();
        let instances = crate::instances::get_instances(
            &self.kube_client,
            &request.challenge_id,
            &request.actor,
        )
        .await;
        let mut success = false;
        for (instance_name, state) in instances {
            if state == InstanceState::Terminating {
                continue;
            }
            crate::instances::delete_instance(
                &self.kube_client,
                &request.challenge_id,
                &request.actor,
                &instance_name,
            )
            .await
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to stop challenge instance {} for challenge {}: {}",
                    instance_name, request.challenge_id, e
                ))
            })?;
            success = true;
        }
        Ok(Response::new(StopChallengeInstanceResponse { success }))
    }

    /// GetChallengeInstanceStatus retrieves the status of a challenge instance for the given team.
    async fn get_challenge_instance_status(
        &self,
        request: tonic::Request<GetChallengeInstanceStatusRequest>,
    ) -> Result<tonic::Response<GetChallengeInstanceStatusResponse>, tonic::Status> {
        let request = request.into_inner();
        let instances = crate::instances::get_instances(
            &self.kube_client,
            &request.challenge_id,
            &request.actor,
        )
        .await
        .into_iter()
        .filter(|(_, state)| *state != InstanceState::Terminating)
        .collect::<HashMap<_, _>>();
        if instances.is_empty() {
            return Ok(Response::new(GetChallengeInstanceStatusResponse {
                is_deployed: false,
                is_ready: false,
                connection_info: vec![],
            }));
        }
        // For simplicity, we assume only one instance per challenge per actor
        let (instance_id, state) = instances.into_iter().next().unwrap();
        let is_ready = match state {
            InstanceState::Running => true,
            _ => false,
        };
        let challenge =
            load_challenge_from_repo(&self.repo_dir, &request.challenge_id, &request.actor, false)
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to load challenge {} from repo: {}",
                        request.challenge_id, e
                    ))
                })?;
        let connection_info =
            get_connection_details(&challenge.compose, &request.challenge_id, &instance_id);
        Ok(Response::new(GetChallengeInstanceStatusResponse {
            is_deployed: true,
            is_ready,
            connection_info,
        }))
    }

    /// CheckFlag verifies if the provided flag is correct for the specified challenge and team.
    async fn check_flag(
        &self,
        request: tonic::Request<CheckFlagRequest>,
    ) -> Result<tonic::Response<CheckFlagResponse>, tonic::Status> {
        let request = request.into_inner();
        let challenges = if let Some(challenge_id) = request.challenge_id {
            let challenge =
                load_challenge_from_repo(&self.repo_dir, &challenge_id, &request.actor, false)
                    .await
                    .map_err(|e| {
                        tonic::Status::internal(format!(
                            "Failed to load challenge {} from repo: {}",
                            challenge_id, e
                        ))
                    })?;
            HashMap::from([(challenge_id, challenge)])
        } else {
            load_challenges_from_repo(&self.repo_dir, &request.actor, false)
                .await
                .map_err(|e| tonic::Status::internal(format!("Failed to load challenges: {}", e)))?
        };
        let mut solved_challenge_id = None;
        let total_challs = challenges.len();
        for (challenge_id, chall) in challenges {
            match chall.metadata.check_flag(&request.flag).map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to check flag for challenge {}: {}",
                    challenge_id, e
                ))
            }) {
                Ok(true) => {
                    solved_challenge_id = Some(challenge_id);
                    break;
                }
                Ok(false) => continue,
                Err(e) => {
                    if total_challs == 1 {
                        return Err(e);
                    } else {
                        tracing::error!(
                            "Error checking flag for challenge {}: {}",
                            challenge_id,
                            e
                        );
                        continue;
                    }
                }
            }
        }
        Ok(Response::new(CheckFlagResponse {
            solved_challenge_id,
        }))
    }

    async fn export_challenge(
        &self,
        request: tonic::Request<ExportChallengeRequest>,
    ) -> Result<tonic::Response<ExportChallengeResponse>, tonic::Status> {
        let request = request.into_inner();
        let challenge =
            load_challenge_from_repo(&self.repo_dir, &request.challenge_id, &request.actor, true)
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to load challenge {} from repo: {}",
                        request.challenge_id, e
                    ))
                })?;
        if !challenge.metadata.auto_publish_src {
            return Err(tonic::Status::permission_denied(format!(
                "Challenge {} is not allowed to be exported",
                request.challenge_id
            )));
        }
        if request.require_release {
            let now = chrono::Utc::now().timestamp() as u64;
            if let Some(release_time) = challenge.metadata.release_time {
                if now < release_time {
                    return Err(tonic::Status::permission_denied(format!(
                        "Challenge {} has not been released yet",
                        request.challenge_id
                    )));
                }
            }
        }
        let packed_data = challenge.export.ok_or_else(|| {
            tonic::Status::internal(format!(
                "Challenge {} does not have export data",
                request.challenge_id
            ))
        })?;
        Ok(Response::new(ExportChallengeResponse {
            challenge_archive: packed_data,
        }))
    }

    async fn retrieve_file(
        &self,
        request: tonic::Request<RetrieveFileRequest>,
    ) -> Result<tonic::Response<RetrieveFileResponse>, tonic::Status> {
        let request = request.into_inner();
        let challenge =
            load_challenge_from_repo(&self.repo_dir, &request.challenge_id, &request.actor, true)
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to load challenge {} from repo: {}",
                        request.challenge_id, e
                    ))
                })?;
        if request.require_release {
            let now = chrono::Utc::now().timestamp() as u64;
            if let Some(release_time) = challenge.metadata.release_time {
                if now < release_time {
                    return Err(tonic::Status::permission_denied(format!(
                        "Challenge {} has not been released yet",
                        request.challenge_id
                    )));
                }
            }
        }
        let working_dir = tempfile::tempdir().map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to create temporary working directory: {}",
                e
            ))
        })?;
        render_dir_recursively(
            &self.repo_dir.join("challs").join(&request.challenge_id),
            &working_dir.path(),
            &request.actor,
            true,
        )
        .map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to render challenge templates for challenge {}: {}",
                request.challenge_id, e
            ))
        })?;
        if !challenge.metadata.attachments.contains(&request.filename) {
            return Err(tonic::Status::not_found(format!(
                "File {} not found in challenge {}",
                request.filename, request.challenge_id
            )));
        }
        let file_path = working_dir.path().join(&request.filename);
        let file_content = std::fs::read(&file_path).map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to read file {} for challenge {}: {}",
                request.filename, request.challenge_id, e
            ))
        })?;
        Ok(Response::new(crate::grpc::api::RetrieveFileResponse {
            file_content,
        }))
    }
}
