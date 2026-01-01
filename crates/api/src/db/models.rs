// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use diesel::associations::Identifiable;
use diesel::prelude::*;
use juniper::GraphQLEnum;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::schema::*;

#[derive(
    diesel_derive_enum::DbEnum,
    Debug,
    PartialEq,
    Eq,
    Deserialize,
    Serialize,
    Clone,
    Copy,
    Ord,
    PartialOrd,
    GraphQLEnum,
)]
#[DbValueStyle = "UPPERCASE"]
#[ExistingTypePath = "crate::db::schema::sql_types::UserRole"]
pub enum UserRole {
    Player,
    Author,
    Admin,
}

/* =========================
 * USERS
 * ========================= */

#[derive(Queryable, Selectable, Identifiable, Debug)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]

pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub email: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub team_id: Option<Uuid>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub email: String,
    pub role: UserRole,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub team_id: Option<Uuid>,
}

/* =========================
 * SESSIONS
 * ========================= */

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(table_name = sessions)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Session {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub user_agent: Option<String>,
    pub ip_address: Option<ipnet::IpNet>,
    pub session_token: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = sessions)]
pub struct NewSession {
    pub user_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub user_agent: Option<String>,
    pub ip_address: Option<ipnet::IpNet>,
    pub session_token: String,
}

/* =========================
 * TEAMS
 * ========================= */

#[derive(Queryable, Selectable, Identifiable, Debug)]
#[diesel(table_name = teams)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub join_code: Option<String>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = teams)]
pub struct NewTeam {
    pub name: String,
    pub slug: String,
    pub join_code: Option<String>,
}

/* =========================
 * SOLVES
 * ========================= */

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(table_name = solves)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Solve {
    pub id: Uuid,
    pub user_id: Uuid,
    pub challenge_id: String,
    pub solved_at: DateTime<Utc>,
    pub submitted_flag: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = solves)]
pub struct NewSolve {
    pub user_id: Uuid,
    pub challenge_id: String,
    pub submitted_flag: String,
    pub solved_at: DateTime<Utc>,
}

/* =========================
 * INVALID SUBMISSIONS
 * ========================= */

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(table_name = invalid_submissions)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct InvalidSubmission {
    pub id: Uuid,
    pub user_id: Uuid,
    pub challenge_id: String,
    pub submitted_flag: String,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = invalid_submissions)]
pub struct NewInvalidSubmission {
    pub user_id: Uuid,
    pub challenge_id: String,
    pub submitted_flag: String,
    pub submitted_at: DateTime<Utc>,
}

/* =========================
 * TEAM INVITATIONS
 * ========================= */

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(table_name = team_invitations)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Team))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TeamInvitation {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub invited_at: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = team_invitations)]
pub struct NewTeamInvitation {
    pub user_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
}

/* =========================
 * TEAM JOIN REQUESTS
 * ========================= */

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(table_name = team_join_requests)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Team))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TeamJoinRequest {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub requested_at: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = team_join_requests)]
pub struct NewTeamJoinRequest {
    pub user_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
}
