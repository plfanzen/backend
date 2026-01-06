// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::{FieldResult, graphql_object};

use crate::graphql::handlers::{self, sessions::SessionCredentials};

use super::Context;

pub struct Mutation;

#[graphql_object]
#[graphql(
    context = Context,
)]
impl Mutation {
    async fn login(
        context: &Context,
        username: String,
        password: String,
    ) -> FieldResult<SessionCredentials> {
        handlers::users::login_user(username, password, context).await
    }

    async fn create_user(
        context: &Context,
        username: String,
        email: String,
        password: String,
    ) -> FieldResult<bool> {
        handlers::users::create_user(username, email, password, context).await
    }

    async fn refresh_session(
        context: &Context,
        refresh_token: String,
    ) -> FieldResult<SessionCredentials> {
        handlers::sessions::refresh_session(context, refresh_token).await
    }

    async fn end_session(context: &Context, refresh_token: String) -> FieldResult<bool> {
        handlers::sessions::end_session(context, refresh_token).await
    }

    async fn sync_repo(context: &Context) -> FieldResult<bool> {
        handlers::repo::sync_repository(context).await
    }

    async fn launch_challenge_instance(
        context: &Context,
        challenge_id: String,
    ) -> FieldResult<bool> {
        handlers::challenges::instances::launch_challenge_instance(context, challenge_id).await
    }

    async fn stop_challenge_instance(context: &Context, challenge_id: String) -> FieldResult<bool> {
        handlers::challenges::instances::stop_challenge_instance(context, challenge_id).await
    }

    /// Returns the ID of the solved challenge if the flag is correct, or null otherwise.
    async fn submit_flag(
        context: &Context,
        challenge_id: String,
        flag: String,
    ) -> FieldResult<Option<String>> {
        handlers::challenges::flags::submit_flag(context, challenge_id, flag).await
    }

    async fn join_team_with_code(
        context: &Context,
        join_code_input: String,
    ) -> FieldResult<crate::db::models::Team> {
        handlers::teams::join_team_with_code(context, join_code_input).await
    }

    async fn create_team(
        context: &Context,
        name: String,
        slug: String,
        create_join_code: bool,
    ) -> FieldResult<crate::db::models::Team> {
        handlers::teams::create_team(context, name, slug, create_join_code).await
    }

    async fn leave_team(context: &Context) -> FieldResult<bool> {
        handlers::teams::leave_team(context).await
    }

    async fn enable_join_code(context: &Context) -> FieldResult<String> {
        handlers::teams::enable_join_code(context).await
    }

    async fn disable_join_code(context: &Context) -> FieldResult<bool> {
        handlers::teams::disable_join_code(context).await
    }
}
