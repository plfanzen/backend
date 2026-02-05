// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::graphql_object;

use crate::{
    db::models::{InvalidSubmission, User},
    graphql::handlers::owned_resource::{HasActor, HasOwnerUserId},
};

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

impl HasOwnerUserId for InvalidSubmission {
    fn user_id(&self) -> uuid::Uuid {
        self.user_id
    }
}

// This does not implement permission checks itself - that is expected to be done
// by whatever exposes this in the GraphQL schema.
#[graphql_object]
impl InvalidSubmission {
    pub fn challenge_id(&self) -> &str {
        &self.challenge_id
    }

    pub fn submitted_flag(&self) -> &str {
        &self.submitted_flag
    }

    pub fn submitted_at(&self) -> String {
        self.submitted_at.to_rfc3339()
    }

    pub async fn user(&self, ctx: &crate::graphql::Context) -> juniper::FieldResult<User> {
        use crate::db::schema::users::dsl::*;
        let user_record = users
            .filter(id.eq(self.user_id))
            .first::<User>(&mut ctx.get_db_conn().await)
            .await?;
        Ok(user_record)
    }

    pub async fn challenge(
        &self,
        ctx: &crate::graphql::Context,
    ) -> juniper::FieldResult<crate::graphql::handlers::challenges::CtfChallengeMetadata> {
        use crate::graphql::handlers::challenges::get_challenges_for_actor;

        let actor = self.actor(ctx).await?;
        let challenges = get_challenges_for_actor(ctx, actor).await?;

        challenges
            .into_iter()
            .find(|c| c.id == self.challenge_id)
            .ok_or_else(|| juniper::FieldError::new("Challenge not found", juniper::Value::null()))
    }
}
