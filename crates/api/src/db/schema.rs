// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "user_role"))]
    pub struct UserRole;
}

diesel::table! {
    invalid_submissions (id) {
        id -> Uuid,
        user_id -> Uuid,
        challenge_id -> Varchar,
        submitted_flag -> Varchar,
        submitted_at -> Timestamptz,
    }
}

diesel::table! {
    sessions (id) {
        id -> Uuid,
        user_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
        expires_at -> Timestamptz,
        user_agent -> Nullable<Varchar>,
        ip_address -> Nullable<Inet>,
        session_token -> Varchar,
    }
}

diesel::table! {
    solves (id) {
        id -> Uuid,
        user_id -> Uuid,
        challenge_id -> Varchar,
        solved_at -> Timestamptz,
        submitted_flag -> Varchar,
    }
}

diesel::table! {
    teams (id) {
        id -> Uuid,
        name -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        join_code -> Nullable<Varchar>,
        #[max_length = 255]
        slug -> Varchar,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::UserRole;

    users (id) {
        id -> Uuid,
        username -> Varchar,
        display_name -> Varchar,
        password_hash -> Varchar,
        email -> Varchar,
        role -> UserRole,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        email_verified_at -> Nullable<Timestamptz>,
        is_active -> Bool,
        team_id -> Nullable<Uuid>,
    }
}

diesel::joinable!(invalid_submissions -> users (user_id));
diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(solves -> users (user_id));
diesel::joinable!(users -> teams (team_id));

diesel::allow_tables_to_appear_in_same_query!(
    invalid_submissions,
    sessions,
    solves,
    teams,
    users,
);
