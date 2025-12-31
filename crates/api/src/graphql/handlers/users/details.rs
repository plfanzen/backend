// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::{FieldResult, graphql_object};

use crate::db::models::{User, UserRole};
use crate::graphql::Context;

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[graphql_object]
impl User {
    pub fn id(&self) -> String {
        self.id.to_string()
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn email(&self, ctx: &Context) -> FieldResult<String> {
        if ctx
            .user
            .as_ref()
            .is_some_and(|u| u.user_id == self.id || u.role == crate::db::models::UserRole::Admin)
        {
            Ok(self.email.clone())
        } else {
            Err(juniper::FieldError::new(
                "Permission denied to view email",
                juniper::Value::null(),
            ))
        }
    }

    pub fn role(&self) -> UserRole {
        self.role
    }
    
    pub async fn invalid_submissions_count(&self, ctx: &Context) -> FieldResult<i32> {
        ctx.require_role_min(UserRole::Author)?;
        use crate::db::schema::invalid_submissions::dsl::*;
        let count: i64 = invalid_submissions
            .filter(user_id.eq(self.id))
            .count()
            .get_result(&mut ctx.get_db_conn().await)
            .await?;
        Ok(count as i32)
    }
    
    pub async fn solves_count(&self, ctx: &Context) -> FieldResult<i32> {
        use crate::db::schema::solves::dsl::*;
        let count: i64 = solves
            .filter(user_id.eq(self.id))
            .count()
            .get_result(&mut ctx.get_db_conn().await)
            .await?;
        Ok(count as i32)
    }
    
    pub async fn invalid_submissions(&self, ctx: &Context) -> FieldResult<Vec<crate::db::models::InvalidSubmission>> {
        if !ctx.user.as_ref().is_some_and(|u| u.user_id == self.id || u.role == UserRole::Admin) {
            return Err(juniper::FieldError::new(
                "Permission denied to view invalid submissions",
                juniper::Value::null(),
            ));
        }
        use crate::db::schema::invalid_submissions::dsl::*;
        let records = invalid_submissions
            .filter(user_id.eq(self.id))
            .load::<crate::db::models::InvalidSubmission>(&mut ctx.get_db_conn().await)
            .await?;
        Ok(records)
    }
    
    pub async fn solves(&self, ctx: &Context) -> FieldResult<Vec<crate::db::models::Solve>> {
        use crate::db::schema::solves::dsl::*;
        let records = solves
            .filter(user_id.eq(self.id))
            .load::<crate::db::models::Solve>(&mut ctx.get_db_conn().await)
            .await?;
        Ok(records)
    }
    
    pub fn actor(&self) -> String {
        if self.team_id.is_some() {
            format!("team-{}", self.team_id.unwrap())
        } else {
            format!("user-{}", self.id)
        }
    }
}
