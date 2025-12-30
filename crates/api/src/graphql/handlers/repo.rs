// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::graphql::Context;
use juniper::GraphQLObject;

#[derive(GraphQLObject)]
pub struct SyncStatus {
    pub commit_hash: Option<String>,
    // This will only work until 2038, but that's probably fine for now :)
    // This is a limitation of GraphQL's lack of unsigned integers / 64-bit integers.
    pub commit_timestamp: Option<i32>,
    pub commit_author: Option<String>,
    pub commit_title: Option<String>,
    pub is_synced: bool,
}

pub async fn get_sync_status(context: &Context) -> juniper::FieldResult<SyncStatus> {
    context.require_role_min(crate::db::models::UserRole::Author)?;

    let mut client = context.repo_client();

    let request = tonic::Request::new(crate::manager_api::GetSyncStatusRequest {});

    let response = client.get_sync_status(request).await?;

    let sync_status = response.into_inner().sync_status;

    match sync_status {
        None => Ok(SyncStatus {
            commit_hash: None,
            commit_timestamp: None,
            commit_author: None,
            commit_title: None,
            is_synced: false,
        }),
        Some(status) => Ok(SyncStatus {
            commit_hash: Some(status.commit_hash),
            commit_timestamp: Some(status.commit_timestamp as i32),
            commit_author: Some(status.commit_author),
            commit_title: Some(status.commit_title),
            is_synced: true,
        }),
    }
}

pub async fn sync_repository(context: &Context) -> juniper::FieldResult<bool> {
    context.require_role_min(crate::db::models::UserRole::Admin)?;

    let mut client = context.repo_client();

    let request = tonic::Request::new(crate::manager_api::SyncChallengesRequest {});

    let _response = client.sync_challenges(request).await?;

    Ok(true)
}
