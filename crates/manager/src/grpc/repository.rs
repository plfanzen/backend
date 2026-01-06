// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use crate::{
    grpc::api::{
        EventConfiguration, GetBuildStatusRequest, GetBuildStatusResponse,
        GetEventConfigurationRequest, GetSyncStatusRequest, GetSyncStatusResponse,
        SyncChallengesRequest, SyncChallengesResponse, SyncStatus,
    },
    repo::EventConfig,
};

use super::api::repository_service_server::RepositoryService;
pub struct RepoManager {
    pub repo_dir: PathBuf,
    pub git_url: String,
    pub git_branch: String,
}

#[tonic::async_trait]
impl RepositoryService for RepoManager {
    /// SyncChallenges pulls the latest changes from the remote challenge repository.
    async fn sync_challenges(
        &self,
        _request: tonic::Request<SyncChallengesRequest>,
    ) -> Result<tonic::Response<SyncChallengesResponse>, tonic::Status> {
        crate::repo::sync_repo(&self.repo_dir, &self.git_url, &self.git_branch)
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to sync repository: {}", e)))?;
        let commit_info = crate::repo::get_head_commit_info(&self.repo_dir).ok_or_else(|| {
            tonic::Status::internal("Failed to get head commit info after syncing")
        })?;
        Ok(tonic::Response::new(SyncChallengesResponse {
            success: true,
            sync_status: Some(SyncStatus {
                commit_hash: commit_info.hash,
                commit_timestamp: commit_info.timestamp,
                commit_author: commit_info.author,
                commit_title: commit_info.title,
            }),
        }))
    }

    /// GetBuildStatus retrieves the build status of all challenges.
    async fn get_build_status(
        &self,
        _request: tonic::Request<GetBuildStatusRequest>,
    ) -> Result<tonic::Response<GetBuildStatusResponse>, tonic::Status> {
        todo!()
    }

    /// GetEventConfiguration retrieves the event configuration from the repository.
    async fn get_event_configuration(
        &self,
        _request: tonic::Request<GetEventConfigurationRequest>,
    ) -> Result<tonic::Response<EventConfiguration>, tonic::Status> {
        let config = EventConfig::try_load_from_repo(&self.repo_dir)
            .await
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to load event configuration from repository: {}",
                    e
                ))
            })?;
        Ok(tonic::Response::new(EventConfiguration {
            event_name: config.event_name,
            front_page_md: config.front_page_md,
            rules_md: config.rules_md,
            start_time: config.start_time.timestamp() as u64,
            end_time: config.end_time.timestamp() as u64,
            use_teams: config.use_teams,
            max_team_size: config.max_team_size,
            scoreboard_freeze_time: config.scoreboard_freeze_time.map(|t| t.timestamp() as u64),
            registration_start_time: config.registration_start_time.map(|t| t.timestamp() as u64),
            registration_end_time: config.registration_end_time.map(|t| t.timestamp() as u64),
            categories: config
                .categories
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        crate::grpc::api::CtfCategory {
                            name: v.name,
                            description: v.description,
                            color: v.color,
                        },
                    )
                })
                .collect(),
            difficulties: config
                .difficulties
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        crate::grpc::api::CtfDifficulty {
                            name: v.name,
                            color: v.color,
                        },
                    )
                })
                .collect(),
        }))
    }

    async fn get_sync_status(
        &self,
        _request: tonic::Request<GetSyncStatusRequest>,
    ) -> Result<tonic::Response<GetSyncStatusResponse>, tonic::Status> {
        let sync_status =
            crate::repo::get_head_commit_info(&self.repo_dir).map(|commit_info| SyncStatus {
                commit_hash: commit_info.hash,
                commit_timestamp: commit_info.timestamp,
                commit_author: commit_info.author,
                commit_title: commit_info.title,
            });
        Ok(tonic::Response::new(GetSyncStatusResponse { sync_status }))
    }
}
