// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::graphql_object;

use crate::db::models::{Solve, User, UserRole};

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[graphql_object]
impl Solve {
    pub fn challenge_id(&self) -> &str {
        &self.challenge_id
    }
    
    pub fn submitted_flag(&self, ctx: &crate::graphql::Context) -> juniper::FieldResult<&str> {
        ctx.require_role_min(UserRole::Author)?;
        Ok(&self.submitted_flag)
    }
    
    pub fn solved_at(&self) -> String {
        self.solved_at.to_rfc3339()
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
