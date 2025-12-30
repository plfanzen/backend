// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::graphql_object;

use super::Context;

pub struct Query;

#[graphql_object]
#[graphql(context = Context)]
impl Query {
    fn is_authenticated(context: &Context) -> bool {
        context.is_authenticated()
    }

    async fn sync_status(
        context: &Context,
    ) -> juniper::FieldResult<crate::graphql::handlers::repo::SyncStatus> {
        crate::graphql::handlers::repo::get_sync_status(context).await
    }

    async fn event_config(
        context: &Context,
    ) -> juniper::FieldResult<crate::graphql::handlers::event::EventConfig> {
        crate::graphql::handlers::event::get_event_config(context).await
    }

    async fn challenges(
        context: &Context,
    ) -> juniper::FieldResult<Vec<crate::graphql::handlers::challenges::CtfChallengeMetadata>> {
        crate::graphql::handlers::challenges::get_challenges(context).await
    }
}
