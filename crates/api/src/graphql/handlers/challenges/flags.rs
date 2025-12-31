use crate::{
    db::{
        models::{NewSolve, Solve},
        schema::solves,
    },
    graphql::Context,
    manager_api::CheckFlagRequest,
};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

pub async fn submit_flag(
    context: &Context,
    challenge_id: String,
    flag: String,
) -> juniper::FieldResult<Option<String>> {
    if challenge_id.is_empty()
        || !challenge_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(juniper::FieldError::new(
            "Challenge ID cannot be empty and must be alphanumeric lowercase with dashes or underscores",
            juniper::Value::null(),
        ));
    }
    let ts_now = chrono::Utc::now();
    let user = context.require_authentication()?;

    // TODO: This allows submitting flags for unreleased challenges. I'm not sure if we should fix that.

    let mut challenges_client = context.challenges_client();

    let mut solved_challenge = challenges_client
        .check_flag(CheckFlagRequest {
            actor: user.actor(),
            challenge_id: Some(challenge_id.clone()),
            flag: flag.to_string(),
        })
        .await
        .map(|r| r.into_inner().solved_challenge_id)
        .unwrap_or_else(|e| {
            tracing::error!("Failed to check flag: {}", e);
            None
        });

    if solved_challenge.is_none() {
        solved_challenge = challenges_client
            .check_flag(CheckFlagRequest {
                actor: user.actor(),
                challenge_id: None,
                flag: flag.to_string(),
            })
            .await
            .map(|r| r.into_inner().solved_challenge_id)
            .unwrap_or_else(|e| {
                tracing::error!("Failed to check flag: {}", e);
                None
            });
    }

    if let Some(challenge_id) = solved_challenge {
        let new_submission = NewSolve {
            user_id: user.user_id,
            challenge_id,
            submitted_flag: flag,
            solved_at: ts_now,
        };
        diesel::insert_into(solves::table)
            .values(&new_submission)
            .returning(Solve::as_returning())
            .execute(&mut context.get_db_conn().await)
            .await?;
    } else {
        let new_invalid_submission = crate::db::models::NewInvalidSubmission {
            // This can be unwrap()ed safely because of the authentication check at the start of the function
            user_id: user.user_id,
            challenge_id: challenge_id,
            submitted_flag: flag,
            submitted_at: ts_now,
        };
        diesel::insert_into(crate::db::schema::invalid_submissions::table)
            .values(&new_invalid_submission)
            .execute(&mut context.get_db_conn().await)
            .await?;
    }
    Ok(solved_challenge)
}
