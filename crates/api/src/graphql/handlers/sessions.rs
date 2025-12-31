// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use diesel::prelude::*;
use ed25519_dalek::SigningKey;
use juniper::GraphQLObject;

use crate::{
    db::models::UserRole,
    graphql::{
        Context,
        auth::{AuthJwtPayload, JwtPayload, RefreshJwtPayload, generate_jwt},
    },
};

use diesel_async::RunQueryDsl;

#[derive(GraphQLObject)]
pub struct SessionCredentials {
    pub refresh_token: String,
    pub access_token: String,
}

pub async fn create_session(
    ctx: &Context,
    uid: uuid::Uuid,
    role: UserRole,
    username: String,
    team_id: Option<uuid::Uuid>,
    team_slug: Option<String>,
    key: &SigningKey,
) -> juniper::FieldResult<SessionCredentials> {
    let session_token = uuid::Uuid::now_v7().to_string();
    let access_token = generate_jwt(
        &JwtPayload::new_with_duration(
            uid.clone(),
            vec!["plfanzen".to_string()],
            AuthJwtPayload {
                role,
                username,
                team_id,
                team_slug,
            },
            Duration::from_mins(10),
        ),
        key,
    )?;

    let session = diesel::insert_into(crate::db::schema::sessions::table)
        .values(crate::db::models::NewSession {
            expires_at: chrono::Utc::now() + chrono::Duration::days(7),
            user_agent: Some(ctx.get_user_agent().to_string()),
            ip_address: Some(match ctx.get_ip() {
                // These functions only return an Err() if prefix_len is too long, but the ones here are harcoded
                // Unless the IP standard changes, this will not panic
                std::net::IpAddr::V4(_) => ipnet::IpNet::new(ctx.get_ip().clone(), 32).unwrap(),
                std::net::IpAddr::V6(_) => ipnet::IpNet::new(ctx.get_ip().clone(), 128).unwrap(),
            }),
            session_token: session_token.clone(),
            user_id: Some(uid),
        })
        .get_result::<crate::db::models::Session>(&mut ctx.get_db_conn().await)
        .await?;

    let refresh_token = generate_jwt(
        &JwtPayload::new_with_exp_ts(
            uid,
            vec!["plfanzen-refresh".to_string()],
            RefreshJwtPayload {
                jti: session_token,
                session_id: session.id,
            },
            session.expires_at.timestamp() as usize,
        ),
        key,
    )?;

    Ok(SessionCredentials {
        access_token,
        refresh_token,
    })
}

pub async fn refresh_session(
    ctx: &Context,
    refresh_token: String,
) -> juniper::FieldResult<SessionCredentials> {
    let refresh_token = crate::graphql::auth::parse_and_validate_jwt::<
        crate::graphql::auth::RefreshJwtPayload,
    >(&refresh_token, &ctx.get_signing_key().verifying_key())?;
    let (current_session, user, team) = {
        let mut con = ctx.get_db_conn().await;
        crate::db::schema::sessions::table
            .filter(crate::db::schema::sessions::session_token.eq(&refresh_token.custom_fields.jti))
            .filter(crate::db::schema::sessions::id.eq(refresh_token.custom_fields.session_id))
            .filter(crate::db::schema::sessions::expires_at.gt(chrono::Utc::now()))
            .filter(crate::db::schema::sessions::user_id.eq(&refresh_token.sub))
            .inner_join(crate::db::schema::users::table.on(
                crate::db::schema::sessions::user_id.eq(crate::db::schema::users::id.nullable()),
            ))
            .left_join(
                crate::db::schema::teams::table
                    .on(crate::db::schema::users::team_id
                        .eq(crate::db::schema::teams::id.nullable())),
            )
            .select((
                crate::db::models::Session::as_select(),
                crate::db::models::User::as_select(),
                Option::<crate::db::models::Team>::as_select(),
            ))
            .first::<(
                crate::db::models::Session,
                crate::db::models::User,
                Option<crate::db::models::Team>,
            )>(&mut con)
            .await?
    };
    let new_session_token = uuid::Uuid::now_v7();
    let access_token = generate_jwt(
        &JwtPayload::new_with_duration(
            refresh_token.sub.clone(),
            vec!["plfanzen".to_string()],
            AuthJwtPayload {
                role: user.role,
                username: user.username,
                team_id: user.team_id,
                team_slug: team.map(|t| t.name),
            },
            Duration::from_mins(10),
        ),
        &ctx.get_signing_key(),
    )?;
    let mut con = ctx.get_db_conn().await;
    let new_session = diesel::update(
        crate::db::schema::sessions::table
            .filter(crate::db::schema::sessions::id.eq(current_session.id)),
    )
    .set((
        crate::db::schema::sessions::session_token.eq(new_session_token.to_string()),
        crate::db::schema::sessions::expires_at.eq(chrono::Utc::now() + chrono::Duration::days(7)),
        crate::db::schema::sessions::user_agent.eq(Some(ctx.get_user_agent().to_string())),
        crate::db::schema::sessions::ip_address.eq(Some(match ctx.get_ip() {
            // These functions only return an Err() if prefix_len is too long, but the ones here are harcoded
            // Unless the IP standard changes, this will not panic
            std::net::IpAddr::V4(_) => ipnet::IpNet::new(ctx.get_ip().clone(), 32).unwrap(),
            std::net::IpAddr::V6(_) => ipnet::IpNet::new(ctx.get_ip().clone(), 128).unwrap(),
        })),
    ))
    .get_result::<crate::db::models::Session>(&mut con)
    .await?;
    let new_refresh_token = generate_jwt(
        &JwtPayload::new_with_exp_ts(
            refresh_token.sub,
            vec!["plfanzen-refresh".to_string()],
            RefreshJwtPayload {
                jti: new_session_token.to_string(),
                session_id: current_session.id,
            },
            new_session.expires_at.timestamp() as usize,
        ),
        &ctx.get_signing_key(),
    )?;
    Ok(SessionCredentials {
        access_token,
        refresh_token: new_refresh_token,
    })
}

pub async fn end_session(ctx: &Context, refresh_token: String) -> juniper::FieldResult<bool> {
    let jwt_payload = crate::graphql::auth::parse_and_validate_jwt::<
        crate::graphql::auth::RefreshJwtPayload,
    >(&refresh_token, &ctx.get_signing_key().verifying_key())?;
    let mut con = ctx.get_db_conn().await;
    diesel::delete(
        crate::db::schema::sessions::table
            .filter(crate::db::schema::sessions::id.eq(jwt_payload.custom_fields.session_id))
            .filter(crate::db::schema::sessions::session_token.eq(&jwt_payload.custom_fields.jti))
            .filter(crate::db::schema::sessions::user_id.eq(&jwt_payload.sub)),
    )
    .execute(&mut con)
    .await?;
    Ok(true)
}
