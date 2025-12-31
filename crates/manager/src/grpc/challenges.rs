// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::path::PathBuf;

use tonic::Response;

use crate::grpc::api::{
    Challenge, CheckFlagRequest, CheckFlagResponse, ConnectionInfo,
    GetChallengeInstanceStatusRequest, GetChallengeInstanceStatusResponse, ListChallengesRequest,
    ListChallengesResponse, Protocol, StartChallengeInstanceRequest,
    StartChallengeInstanceResponse, StopChallengeInstanceRequest, StopChallengeInstanceResponse,
};
use crate::instances::InstanceState;
use crate::repo::challenges::loader::{load_challenge_from_repo, load_challenges_from_repo};

use super::api::challenges_service_server::ChallengesService;
pub struct ChallengeManager {
    pub repo_dir: PathBuf,
    pub kube_client: kube::Client,
}

fn get_connection_details(
    challenge: &crate::repo::challenges::manifest::ChallengeYml,
    challenge_id: &str,
    instance_id: &str,
    actor: &str,
) -> Vec<ConnectionInfo> {
    let mut connection_info = vec![];
    for (svc_id, svc) in &challenge.services {
        for exposed_port in &svc.external_ports {
            connection_info.push(ConnectionInfo {
                host: format!(
                    "{}-{}-{}-{}.{}",
                    svc_id,
                    exposed_port.port,
                    challenge_id,
                    instance_id,
                    std::env::var("EXPOSED_DOMAIN").unwrap_or("localhost".to_string())
                ),
                port: 443,
                protocol: match exposed_port.protocol {
                    crate::repo::challenges::manifest::service::Protocol::HTTP => {
                        Protocol::Https.into()
                    }
                    crate::repo::challenges::manifest::service::Protocol::TCP => {
                        Protocol::TcpTls.into()
                    }
                },
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
        let challenges = load_challenges_from_repo(&self.repo_dir, &request.actor)
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to load challenges: {}", e)))?;

        let event_config = crate::repo::EventConfig::try_load_from_repo(&self.repo_dir)
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to load event config: {}", e)))?;
        let mut out_challenges = vec![];
        for (id, c) in challenges {
            let solve_info = request.solved_challenges.get(&id);
            let points = event_config
                .calculate_points(
                    &c.metadata,
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
                name: c.metadata.name,
                description: c.metadata.description_md,
                release_timestamp: c.metadata.release_time,
                end_timestamp: c.metadata.end_time,
                categories: c.metadata.categories,
                authors: c.metadata.authors,
                files: HashMap::new(),
                can_start: !c.services.is_empty(),
                points,
                difficulty: c.metadata.difficulty,
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
            load_challenge_from_repo(&self.repo_dir, &request.challenge_id, &request.actor)
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to load challenge {} from repo: {}",
                        request.challenge_id, e
                    ))
                })?;
        
        if challenge.services.is_empty() {
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
        let connection_info = get_connection_details(
            &challenge,
            &request.challenge_id,
            &instance_id,
            &request.actor,
        );

        crate::instances::deploy::deploy_challenge(
            &self.kube_client,
            &instance_id,
            challenge,
            &std::env::var("EXPOSED_DOMAIN").unwrap_or("localhost".to_string()),
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
            load_challenge_from_repo(&self.repo_dir, &request.challenge_id, &request.actor)
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to load challenge {} from repo: {}",
                        request.challenge_id, e
                    ))
                })?;
        let connection_info = get_connection_details(
            &challenge,
            &request.challenge_id,
            &instance_id,
            &request.actor,
        );
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
            let challenge = load_challenge_from_repo(&self.repo_dir, &challenge_id, &request.actor)
                .await
                .map_err(|e| {
                    tonic::Status::internal(format!(
                        "Failed to load challenge {} from repo: {}",
                        challenge_id, e
                    ))
                })?;
            HashMap::from([(challenge_id, challenge)])
        } else {
            load_challenges_from_repo(&self.repo_dir, &request.actor)
                .await
                .map_err(|e| tonic::Status::internal(format!("Failed to load challenges: {}", e)))?
        };
        let mut solved_challenge_id = None;
        let total_challs = challenges.len();
        for (challenge_id, challenge) in challenges {
            match challenge.metadata.check_flag(&request.flag).map_err(|e| {
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
}
