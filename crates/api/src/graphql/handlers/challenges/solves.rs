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
    
    pub async fn actor(
        &self,
        ctx: &crate::graphql::Context,
    ) -> juniper::FieldResult<String> {
        use crate::db::schema::users::dsl::*;
        use crate::db::schema::teams;
        
        let result = users
            .left_join(teams::table)
            .filter(id.eq(self.user_id))
            .select((username, teams::slug.nullable()))
            .first::<(String, Option<String>)>(&mut ctx.get_db_conn().await)
            .await?;
        
        match result.1 {
            Some(team_name) => Ok(format!("team-{}", team_name)),
            None => Ok(format!("user-{}", result.0)),
        }
    }
    
    pub async fn challenge(
        &self,
        ctx: &crate::graphql::Context,
    ) -> juniper::FieldResult<crate::graphql::handlers::challenges::CtfChallengeMetadata> {
        use crate::graphql::handlers::challenges::get_challenges_for_actor;
        
        let challenges = get_challenges_for_actor(
            ctx,
            self.actor(ctx).await?,
        )
        .await?;
        
        challenges
            .into_iter()
            .find(|c| c.id == self.challenge_id)
            .ok_or_else(|| juniper::FieldError::new(
                "Challenge not found",
                juniper::Value::null(),
            ))
    }
}

pub async fn get_solves(
    ctx: &crate::graphql::Context,
) -> juniper::FieldResult<Vec<Solve>> {
    ctx.require_role_min(UserRole::Author)?;
    use crate::db::schema::solves::dsl::*;
    let solve_records = solves
        .load::<Solve>(&mut ctx.get_db_conn().await)
        .await?;
    Ok(solve_records)
}
