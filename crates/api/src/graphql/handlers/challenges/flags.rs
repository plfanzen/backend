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
use serenity::all::{Builder, ChannelId, CreateMessage, GuildId};

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

    // TODO: This allows submitting flags for unreleased challenges. We should probably fix that.

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

    if let Some(challenge_id) = &solved_challenge {
        let new_submission = NewSolve {
            user_id: user.user_id,
            challenge_id: challenge_id.clone(),
            submitted_flag: flag,
            solved_at: ts_now,
        };
        diesel::insert_into(solves::table)
            .values(&new_submission)
            .returning(Solve::as_returning())
            .execute(&mut context.get_db_conn().await)
            .await?;
        if let Some(discord_solves_channel) = std::env::var("DISCORD_SOLVES_CHANNEL_ID")
            .ok()
            .and_then(|id| id.parse::<u64>().ok())
            && let Some(discord_solves_guild) = std::env::var("DISCORD_SOLVES_GUILD_ID")
                .ok()
                .and_then(|id| id.parse::<u64>().ok())
            && let Some(discord_bot) = crate::discord::get_client().await
        {
            if let Some(ref team) = user.team_slug {
                Builder::execute(
                    CreateMessage::new().content(format!(
                        ":tada: User **{}** from team **{}** just solved challenge **{}**!",
                        user.username, team, challenge_id
                    )),
                    &discord_bot.http,
                    (
                        ChannelId::new(discord_solves_channel),
                        Some(GuildId::new(discord_solves_guild)),
                    ),
                )
                .await?;
            } else {
                Builder::execute(
                    CreateMessage::new().content(format!(
                        ":tada: User **{}** just solved challenge **{}**!",
                        user.username, challenge_id
                    )),
                    &discord_bot.http,
                    (
                        ChannelId::new(discord_solves_channel),
                        Some(GuildId::new(discord_solves_guild)),
                    ),
                )
                .await?;
            }
        }
    } else {
        let new_invalid_submission = crate::db::models::NewInvalidSubmission {
            // This can be unwrap()ed safely because of the authentication check at the start of the function
            user_id: user.user_id,
            challenge_id,
            submitted_flag: flag,
            submitted_at: ts_now,
        };
        diesel::insert_into(crate::db::schema::invalid_submissions::table)
            .values(&new_invalid_submission)
            .execute(&mut context.get_db_conn().await)
            .await?;
        if let Some(discord_client) = crate::discord::get_client().await {
            if let Some(discord_invalid_submissions_channel) =
                std::env::var("DISCORD_PUBLIC_INVALID_SUBMISSIONS_CHANNEL_ID")
                    .ok()
                    .and_then(|id| id.parse::<u64>().ok())
                && let Some(discord_invalid_submissions_guild) =
                    std::env::var("DISCORD_PUBLIC_INVALID_SUBMISSIONS_GUILD_ID")
                        .ok()
                        .and_then(|id| id.parse::<u64>().ok())
            {
                Builder::execute(
                    CreateMessage::new().content(format!(
                        ":warning: User **{}** submitted an invalid flag for challenge **{}**.",
                        user.username, new_invalid_submission.challenge_id
                    )),
                    &discord_client.http,
                    (
                        ChannelId::new(discord_invalid_submissions_channel),
                        Some(GuildId::new(discord_invalid_submissions_guild)),
                    ),
                )
                .await?;
            }
            if let Some(discord_private_invalid_submissions_channel) =
                std::env::var("DISCORD_PRIVATE_INVALID_SUBMISSIONS_CHANNEL_ID")
                    .ok()
                    .and_then(|id| id.parse::<u64>().ok())
                && let Some(discord_private_invalid_submissions_guild) =
                    std::env::var("DISCORD_PRIVATE_INVALID_SUBMISSIONS_GUILD_ID")
                        .ok()
                        .and_then(|id| id.parse::<u64>().ok())
            {
                Builder::execute(
                    CreateMessage::new().content(format!(
                        ":warning: User **{}** submitted an invalid flag for challenge **{}**. Submitted flag: `{}`",
                        user.username, new_invalid_submission.challenge_id, new_invalid_submission.submitted_flag
                    )),
                    &discord_client.http,
                    (ChannelId::new(discord_private_invalid_submissions_channel), Some(GuildId::new(discord_private_invalid_submissions_guild)))
                )
                .await?;
            }
        }
    }
    Ok(solved_challenge)
}
