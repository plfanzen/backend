// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    db::{
        models::{NewUser, User},
        schema::users,
    },
    graphql::{
        Context,
        handlers::{event::get_event_config, sessions::SessionCredentials},
    },
};
use argon2::{
    Argon2, PasswordVerifier,
    password_hash::{PasswordHasher, SaltString},
};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use juniper::FieldResult;
use rand_core::OsRng;

pub async fn create_user(
    username: String,
    email: String,
    password: String,
    context: &Context,
) -> FieldResult<bool> {
    let mut role = crate::db::models::UserRole::Player;
    let user_count = users::table
        .count()
        .get_result::<i64>(&mut context.get_db_conn().await)
        .await?;
    if user_count == 0 {
        role = crate::db::models::UserRole::Admin;
    }
    match get_event_config(context).await {
        Ok(event_config) => {
            if let Some(reg_start_time) = event_config.registration_start_time {
                let now = chrono::Utc::now().timestamp();
                if now < (reg_start_time as i64) {
                    return Err(juniper::FieldError::new(
                        "Registration has not started yet",
                        juniper::Value::null(),
                    ));
                }
            }
            if let Some(reg_end_time) = event_config.registration_end_time {
                let now = chrono::Utc::now().timestamp();
                if now > (reg_end_time as i64) {
                    return Err(juniper::FieldError::new(
                        "Registration has ended",
                        juniper::Value::null(),
                    ));
                }
            }
        }
        Err(_) => {
            if user_count > 0 {
                return Err(juniper::FieldError::new(
                    "Event configuration not found; registration is disabled",
                    juniper::Value::null(),
                ));
            }
        }
    }

    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);

    let new_user = NewUser {
        username: username.clone(),
        display_name: username,
        password_hash: argon2
            .hash_password(password.as_bytes(), &salt)?
            .to_string(),
        email,
        role,
        email_verified_at: None,
        // TODO: implement email verification
        is_active: true,
        team_id: None,
    };

    diesel::insert_into(users::table)
        .values(&new_user)
        .returning(User::as_returning())
        .execute(&mut context.get_db_conn().await)
        .await?;

    Ok(true)
}

pub async fn login_user(
    username: String,
    password: String,
    context: &Context,
) -> juniper::FieldResult<SessionCredentials> {
    let user = crate::db::schema::users::table
        .filter(crate::db::schema::users::username.eq(&username))
        .select(User::as_select())
        .first(&mut context.get_db_conn().await)
        .await
        .optional()?;
    match user {
        Some(user) => {
            let parsed_hash = argon2::PasswordHash::new(&user.password_hash)?;
            if Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok()
            {
                let signing_key = context.get_signing_key();
                let session_credentials = crate::graphql::handlers::sessions::create_session(
                    context,
                    user.id,
                    user.role,
                    &signing_key,
                )
                .await?;
                Ok(session_credentials)
            } else {
                Err(juniper::FieldError::new(
                    "Invalid username or password",
                    juniper::Value::null(),
                ))
            }
        }
        None => Err(juniper::FieldError::new(
            "User not found",
            juniper::Value::null(),
        )),
    }
}
