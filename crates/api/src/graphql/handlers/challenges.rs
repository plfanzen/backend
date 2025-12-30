// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod flags;
pub mod instances;

use std::collections::HashMap;

use juniper::graphql_object;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::{db::models::UserRole, graphql::Context, manager_api::ListChallengesRequest};

#[derive(Debug, Clone)]
pub struct CtfChallengeMetadata {
    /// Unique identifier of the challenge
    pub id: String,
    /// Name of the challenge
    pub name: String,
    /// Authors of the challenge
    pub authors: Vec<String>,
    /// Description of the challenge in Markdown format
    pub description_md: String,
    pub categories: Vec<String>,
    pub difficulty: String,
    // Path to attached files
    pub attachments: Vec<String>,
    pub release_time: Option<i32>,
    pub end_time: Option<i32>,
    pub points: i32,
}

pub async fn get_challenges(context: &Context) -> juniper::FieldResult<Vec<CtfChallengeMetadata>> {
    context.require_authentication()?;

    let mut challenges_client = context.challenges_client();

    let challs = challenges_client
        .list_challenges(ListChallengesRequest {
            actor: "TODO".to_string(),
            solved_challenges: HashMap::new(),
            total_competitors: 100,
        })
        .await?
        .into_inner()
        .challenges;

    let can_see_hidden = context.role().is_some_and(|r| r >= UserRole::Author);
    let current_ts = chrono::Utc::now().timestamp() as u32;

    let result = challs
        .into_iter()
        .filter(|c| can_see_hidden || c.release_timestamp.unwrap_or(0) as u32 <= current_ts)
        .map(|c| CtfChallengeMetadata {
            id: c.id,
            name: c.name,
            authors: c.authors,
            description_md: c.description,
            categories: c.categories,
            difficulty: c.difficulty,
            attachments: c.files.keys().cloned().collect(),
            release_time: c.release_timestamp.map(|t| t as i32),
            end_time: c.end_timestamp.map(|t| t as i32),
            points: c.points as i32,
        })
        .collect();
    Ok(result)
}

#[graphql_object]
impl CtfChallengeMetadata {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn authors(&self) -> &Vec<String> {
        &self.authors
    }
    fn description_md(&self) -> &str {
        &self.description_md
    }
    fn categories(&self) -> &Vec<String> {
        &self.categories
    }
    fn difficulty(&self) -> &str {
        &self.difficulty
    }
    fn attachments(&self) -> &Vec<String> {
        &self.attachments
    }
    fn release_time(&self) -> Option<i32> {
        self.release_time
    }
    fn end_time(&self) -> Option<i32> {
        self.end_time
    }
    fn points(&self) -> i32 {
        self.points
    }

    async fn instance(
        &self,
        context: &Context,
    ) -> juniper::FieldResult<Option<instances::InstanceStatus>> {
        instances::get_challenge_instance_status(context, self.id.clone()).await
    }
    
    async fn solved(
        &self,
        context: &Context,
    ) -> juniper::FieldResult<bool> {
        let Ok(user) = context.require_authentication() else {
            return Ok(false);
        };

        // Check if there is a solve record for this user (or their team) and this challenge
        let conn = &mut context.get_db_conn().await;
        
        use crate::db::schema::solves::dsl::*;
        
        let solve_count = if let Some(team_id_val) = user.team_id {
            solves
                .filter(challenge_id.eq(&self.id))
                .filter(user_id.nullable().eq_any(
                    crate::db::schema::users::table
                        .filter(crate::db::schema::users::team_id.eq(team_id_val))
                        .select(crate::db::schema::users::id.nullable()),
                ))
                .count()
                .get_result::<i64>(conn)
                .await?
        } else {
            solves
                .filter(challenge_id.eq(&self.id))
                .filter(user_id.eq(user.user_id))
                .count()
                .get_result::<i64>(conn)
                .await?
        };
        
        Ok(solve_count > 0)
    }
    
    async fn solves(
        &self,
        context: &Context,
    ) -> juniper::FieldResult<i32> {
        let conn = &mut context.get_db_conn().await;
        
        use crate::db::schema::solves::dsl::*;
        
        let solve_count = solves
            .filter(challenge_id.eq(&self.id))
            .count()
            .get_result::<i64>(conn)
            .await?;
        
        Ok(solve_count as i32)
    }
}
