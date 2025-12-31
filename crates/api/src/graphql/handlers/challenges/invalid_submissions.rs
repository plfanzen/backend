// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::graphql_object;

use crate::db::models::{InvalidSubmission, User};

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

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
}
