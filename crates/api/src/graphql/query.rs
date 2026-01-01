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

    async fn users(context: &Context) -> juniper::FieldResult<Vec<crate::db::models::User>> {
        crate::graphql::handlers::users::get_all_users(context).await
    }

    async fn me(context: &Context) -> juniper::FieldResult<Option<crate::db::models::User>> {
        crate::graphql::handlers::users::get_current_user(context).await
    }

    async fn user_by_id(
        context: &Context,
        user_id: String,
    ) -> juniper::FieldResult<Option<crate::db::models::User>> {
        let user_id = uuid::Uuid::parse_str(&user_id)?;
        crate::graphql::handlers::users::get_user_by_id(user_id, context).await
    }

    async fn solves(context: &Context) -> juniper::FieldResult<Vec<crate::db::models::Solve>> {
        crate::graphql::handlers::challenges::solves::get_solves(context).await
    }
    
    async fn teams(context: &Context) -> juniper::FieldResult<Vec<crate::db::models::Team>> {
        crate::graphql::handlers::teams::get_teams(context).await
    }
}
