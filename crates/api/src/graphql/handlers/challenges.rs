// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod export;
pub mod flags;
pub mod instances;
pub mod invalid_submissions;
pub mod solves;

use std::collections::HashMap;

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use juniper::graphql_object;

use crate::{
    db::models::UserRole,
    graphql::{Actor, Context},
    manager_api::{
        ListChallengesRequest, SolvedChallenge, challenges_service_client::ChallengesServiceClient,
    },
};

#[derive(QueryableByName)]
struct SolveRankResult {
    #[diesel(sql_type = diesel::sql_types::Text)]
    challenge_id: String,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    solve_rank: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    total_solves: i64,
}

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
    /// Whether the user can start an instance of this challenge
    pub can_start: bool,
    pub can_export: bool,
}

async fn get_actor_solves(
    actor_details: Actor,
    db_pool: diesel_async::pooled_connection::bb8::Pool<diesel_async::AsyncPgConnection>,
) -> juniper::FieldResult<HashMap<String, SolvedChallenge>> {
    let mut conn = db_pool.get().await?;

    let actor_solves = match actor_details {
        Actor::User { id: uid, .. } => {
            diesel::sql_query(
                "WITH first_solves AS (
                    SELECT challenge_id, user_id, MIN(solved_at) as first_solve_at
                    FROM solves
                    GROUP BY challenge_id, user_id
                ),
                user_ranks AS (
                    SELECT 
                        challenge_id,
                        user_id,
                        ROW_NUMBER() OVER (PARTITION BY challenge_id ORDER BY first_solve_at ASC) as solve_rank,
                        COUNT(*) OVER (PARTITION BY challenge_id) as total_solves
                    FROM first_solves
                )
                SELECT challenge_id, solve_rank, total_solves
                FROM user_ranks
                WHERE user_id = $1"
            )
            .bind::<diesel::sql_types::Uuid, _>(uid)
            .load::<SolveRankResult>(&mut conn)
            .await?
        }
        Actor::Team { id: team_id, .. } => {
            diesel::sql_query(
                "WITH team_first_solves AS (
                    SELECT s.challenge_id, u.team_id, MIN(s.solved_at) as first_solve_at
                    FROM solves s
                    INNER JOIN users u ON s.user_id = u.id
                    WHERE u.team_id IS NOT NULL
                    GROUP BY s.challenge_id, u.team_id
                ),
                team_ranks AS (
                    SELECT 
                        challenge_id,
                        team_id,
                        ROW_NUMBER() OVER (PARTITION BY challenge_id ORDER BY first_solve_at ASC) as solve_rank,
                        COUNT(*) OVER (PARTITION BY challenge_id) as total_solves
                    FROM team_first_solves
                )
                SELECT challenge_id, solve_rank, total_solves
                FROM team_ranks
                WHERE team_id = $1"
            )
            .bind::<diesel::sql_types::Uuid, _>(team_id)
            .load::<SolveRankResult>(&mut conn)
            .await?
        }
    };

    let mut result: HashMap<String, SolvedChallenge> = HashMap::new();
    for row in actor_solves {
        result.insert(
            row.challenge_id.clone(),
            SolvedChallenge {
                actor_nth_solve: row.solve_rank as i32,
                total_solves: row.total_solves as i32,
            },
        );
    }

    Ok(result)
}

async fn get_challenges_for_actor_internal(
    db_pool: &diesel_async::pooled_connection::bb8::Pool<diesel_async::AsyncPgConnection>,
    mut challs_client: ChallengesServiceClient<tonic::transport::Channel>,
    current_role: Option<UserRole>,
    actor: Actor,
    total_competitors: i32,
) -> juniper::FieldResult<Vec<CtfChallengeMetadata>> {
    let actor_str = actor.slug();
    let solves = get_actor_solves(actor, db_pool.clone()).await?;
    let challs = challs_client
        .list_challenges(ListChallengesRequest {
            actor: actor_str,
            solved_challenges: solves,
            total_competitors: total_competitors as u64,
            require_release: current_role.is_none() || current_role.unwrap() < UserRole::Author,
        })
        .await?
        .into_inner()
        .challenges;

    let can_see_hidden = current_role.is_some_and(|r| r >= UserRole::Author);
    let current_ts = chrono::Utc::now().timestamp() as u32;

    let result = challs
        .into_iter()
        .filter(|c| can_see_hidden || (c.release_timestamp.unwrap_or(0) as u32) <= current_ts)
        .map(|c| CtfChallengeMetadata {
            id: c.id,
            name: c.name,
            authors: c.authors,
            description_md: c.description,
            categories: c.categories,
            difficulty: c.difficulty,
            attachments: c.attachments,
            release_time: c.release_timestamp.map(|t| t as i32),
            end_time: c.end_timestamp.map(|t| t as i32),
            points: c.points as i32,
            can_start: c.can_start,
            can_export: c.can_export,
        })
        .collect();
    Ok(result)
}

pub async fn get_challenges_for_actor(
    context: &Context,
    actor: Actor,
) -> juniper::FieldResult<Vec<CtfChallengeMetadata>> {
    let current_role = context.user.as_ref().map(|u| u.role);
    let challenges_client = context.challenges_client();
    let total_competitors = context.total_competitors;
    context
        .challenges_cache
        .get_with(actor.slug(), async {
            get_challenges_for_actor_internal(
                &context.base.db_pool,
                challenges_client,
                current_role,
                actor,
                total_competitors,
            )
            .await
        })
        .await
}

pub async fn get_challenges(context: &Context) -> juniper::FieldResult<Vec<CtfChallengeMetadata>> {
    let auth = context.require_authentication()?;
    get_challenges_for_actor(context, auth.actor_details()).await
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

    fn can_start(&self) -> bool {
        self.can_start
    }

    async fn instance(
        &self,
        context: &Context,
    ) -> juniper::FieldResult<Option<instances::InstanceStatus>> {
        instances::get_challenge_instance_status(context, self.id.clone()).await
    }

    async fn solved(&self, context: &Context) -> juniper::FieldResult<bool> {
        let Ok(user) = context.require_authentication() else {
            return Ok(false);
        };

        // Check if there is a solve record for this user (or their team) and this challenge
        let conn = &mut context.get_db_conn().await;

        use crate::db::schema::solves::dsl::*;

        let solve_count = if let Some(team_id_val) = user.team_id {
            solves
                .filter(challenge_id.eq(&self.id))
                .filter(
                    user_id.nullable().eq_any(
                        crate::db::schema::users::table
                            .filter(crate::db::schema::users::team_id.eq(team_id_val))
                            .select(crate::db::schema::users::id.nullable()),
                    ),
                )
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

    /// Whether the challenge source code can be exported by the user
    fn can_export(&self) -> bool {
        self.can_export
    }

    async fn solves(&self, context: &Context) -> juniper::FieldResult<i32> {
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
