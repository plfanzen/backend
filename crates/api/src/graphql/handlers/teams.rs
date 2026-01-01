// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use juniper::graphql_object;

use crate::db::models::{Team, User};

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[graphql_object]
impl Team {
    pub fn id(&self) -> String {
        self.id.to_string()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn slug(&self) -> &str {
        &self.slug
    }
    
    pub fn join_code(&self, ctx: &crate::graphql::Context) -> juniper::FieldResult<Option<&str>> {
        if ctx.user.as_ref().is_some_and(|u| u.role == crate::db::models::UserRole::Admin || u.team_id == Some(self.id)) {
            Ok(self.join_code.as_deref())
        } else {
            Err(juniper::FieldError::new(
                "Permission denied to view join code",
                juniper::Value::null(),
            ))
        }
    }

    pub async fn members(&self, ctx: &crate::graphql::Context) -> juniper::FieldResult<Vec<User>> {
        use crate::db::schema::users::dsl::*;
        let member_records = users
            .filter(team_id.eq(self.id))
            .load::<User>(&mut ctx.get_db_conn().await)
            .await?;
        Ok(member_records)
    }
}

pub async fn join_team_with_code(
    ctx: &crate::graphql::Context,
    join_code_input: String,
) -> juniper::FieldResult<Team> {
    let current_user = ctx.require_authentication()?;

    if current_user.team_id.is_some() {
        return Err(juniper::FieldError::new(
            "User is already in a team",
            juniper::Value::null(),
        ));
    }

    let team_record = {
        use crate::db::schema::teams::dsl::*;

        teams
            .filter(join_code.eq(&join_code_input))
            .select(Team::as_select())
            .first::<Team>(&mut ctx.get_db_conn().await)
            .await?
    };

    {
        use crate::db::schema::users::dsl::*;

        diesel::update(users.filter(id.eq(current_user.user_id)))
            .set(
                team_id.eq(team_record.id),
            )
            .execute(&mut ctx.get_db_conn().await)
            .await?;
    }

    Ok(team_record)
}

pub async fn create_team(
    ctx: &crate::graphql::Context,
    name: String,
    slug: String,
    create_join_code: bool,
) -> juniper::FieldResult<Team> {
    let current_user = ctx.require_authentication()?;

    if current_user.team_id.is_some() {
        return Err(juniper::FieldError::new(
            "User is already in a team",
            juniper::Value::null(),
        ));
    }

    let new_team = crate::db::models::NewTeam {
        name,
        slug,
        join_code: if create_join_code {
            use rand::RngCore;
            let mut buf = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut buf);
            Some(buf.iter().map(|b| format!("{:02x}", b)).collect())
        } else {
            None
        },
    };

    let inserted_team = {
        use crate::db::schema::teams::dsl::*;

        diesel::insert_into(teams)
            .values(&new_team)
            .returning(Team::as_returning())
            .get_result(&mut ctx.get_db_conn().await)
            .await?
    };

    {
        use crate::db::schema::users::dsl::*;

        diesel::update(users.filter(id.eq(current_user.user_id)))
            .set(
                team_id.eq(inserted_team.id),
            )
            .execute(&mut ctx.get_db_conn().await)
            .await?;
    }

    Ok(inserted_team)
}

pub async fn leave_team(
    ctx: &crate::graphql::Context,
) -> juniper::FieldResult<bool> {
    let current_user = ctx.require_authentication()?;

    if current_user.team_id.is_none() {
        return Err(juniper::FieldError::new(
            "User is not in a team",
            juniper::Value::null(),
        ));
    }

    {
        use crate::db::schema::users::dsl::*;

        diesel::update(users.filter(id.eq(current_user.user_id)))
            .set(
                team_id.eq::<Option<uuid::Uuid>>(None),
            )
            .execute(&mut ctx.get_db_conn().await)
            .await?;
    }

    {
        use crate::db::schema::teams::dsl as teams_dsl;
        use crate::db::schema::users::dsl as users_dsl;

        let team_id_val = current_user.team_id.unwrap();

        let member_count: i64 = users_dsl::users
            .filter(users_dsl::team_id.eq(team_id_val))
            .count()
            .get_result(&mut ctx.get_db_conn().await)
            .await?;

        if member_count == 0 {
            diesel::delete(teams_dsl::teams.filter(teams_dsl::id.eq(team_id_val)))
                .execute(&mut ctx.get_db_conn().await)
                .await?;
        }
    }

    Ok(true)
}

pub async fn enable_join_code(
    ctx: &crate::graphql::Context,
) -> juniper::FieldResult<String> {
    let current_user = ctx.require_authentication()?;

    let team_id_val = current_user.team_id.ok_or_else(|| juniper::FieldError::new(
        "User is not in a team",
        juniper::Value::null(),
    ))?;

    use crate::db::schema::teams::dsl::*;

    let new_code: String = {
        use rand::RngCore;
        let mut buf = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut buf);
        buf.iter().map(|b| format!("{:02x}", b)).collect()
    };

    diesel::update(teams.filter(id.eq(team_id_val)))
        .set(join_code.eq(Some(new_code.clone())))
        .execute(&mut ctx.get_db_conn().await)
        .await?;

    Ok(new_code)
}

pub async fn disable_join_code(
    ctx: &crate::graphql::Context,
) -> juniper::FieldResult<bool> {
    let current_user = ctx.require_authentication()?;

    let team_id_val = current_user.team_id.ok_or_else(|| juniper::FieldError::new(
        "User is not in a team",
        juniper::Value::null(),
    ))?;

    use crate::db::schema::teams::dsl::*;

    diesel::update(teams.filter(id.eq(team_id_val)))
        .set(join_code.eq::<Option<String>>(None))
        .execute(&mut ctx.get_db_conn().await)
        .await?;

    Ok(true)
}

pub async fn get_teams(
    ctx: &crate::graphql::Context,
) -> juniper::FieldResult<Vec<Team>> {
    let team_records = crate::db::schema::teams::table
        .select(Team::as_select())
        .load::<Team>(&mut ctx.get_db_conn().await)
        .await?;

    Ok(team_records)
}
