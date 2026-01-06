// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::IpAddr;

use juniper::EmptySubscription;
pub use mutation::Mutation;
pub use query::Query;

use crate::{db::models::UserRole, graphql::handlers::challenges::CtfChallengeMetadata};

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use std::time::Duration;

pub mod auth;
mod handlers;
mod mutation;
mod query;

pub use handlers::challenges::export::{export_challenge, retrieve_file};

#[derive(Clone)]
pub struct BaseContext {
    pub grpc_client: tonic::transport::Channel,
    pub db_pool: diesel_async::pooled_connection::bb8::Pool<diesel_async::AsyncPgConnection>,
    pub keypair: ed25519_dalek::SigningKey,
}

pub struct Context {
    base: BaseContext,
    ip: IpAddr,
    user_agent: String,
    user: Option<AuthenticatedUser>,
    challenges_cache:
        moka::future::Cache<String, Result<Vec<CtfChallengeMetadata>, juniper::FieldError>>,
    total_competitors: i32,
}

impl juniper::Context for Context {}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: uuid::Uuid,
    pub role: UserRole,
    pub team_id: Option<uuid::Uuid>,
    pub username: String,
    pub team_slug: Option<String>,
}

impl AuthenticatedUser {
    pub fn actor(&self) -> String {
        match &self.team_slug {
            Some(slug) => format!("team-{slug}"),
            None => format!("user-{}", self.username),
        }
    }
}

#[cached::proc_macro::cached(time = 300, key = "()", convert = "{ }", result = true)]
async fn get_total_competitors(context: &Context) -> juniper::FieldResult<i32> {
    // This chould be optimized, but for now this is fine
    let conn = &mut context.get_db_conn().await;
    use crate::db::schema::teams::dsl::*;
    let team_count: i64 = teams.count().get_result(conn).await?;
    use crate::db::schema::users::dsl::*;
    let user_count: i64 = users.count().get_result(conn).await?;
    if team_count == 0 {
        Ok(user_count as i32)
    } else {
        Ok(team_count as i32)
    }
}

impl Context {
    pub async fn new(
        base: BaseContext,
        ip: IpAddr,
        user_agent: String,
        user_details: Option<AuthenticatedUser>,
    ) -> Self {
        let mut tmp = Self {
            base,
            ip,
            user_agent,
            user: user_details,
            challenges_cache: moka::future::Cache::builder().build(),
            total_competitors: 0,
        };
        tmp.total_competitors = get_total_competitors(&tmp).await.unwrap_or(0);
        tmp
    }

    async fn get_db_conn(
        &self,
    ) -> diesel_async::pooled_connection::bb8::PooledConnection<'_, diesel_async::AsyncPgConnection>
    {
        self.base
            .db_pool
            .get()
            .await
            .expect("Failed to get DB connection")
    }

    fn repo_client(
        &self,
    ) -> crate::manager_api::repository_service_client::RepositoryServiceClient<
        tonic::transport::Channel,
    > {
        crate::manager_api::repository_service_client::RepositoryServiceClient::new(
            self.base.grpc_client.clone(),
        )
    }

    pub fn challenges_client(
        &self,
    ) -> crate::manager_api::challenges_service_client::ChallengesServiceClient<
        tonic::transport::Channel,
    > {
        crate::manager_api::challenges_service_client::ChallengesServiceClient::new(
            self.base.grpc_client.clone(),
        )
    }

    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    pub fn role(&self) -> Option<UserRole> {
        self.user.as_ref().map(|u| u.role.clone())
    }

    pub fn require_role_exact(&self, required_role: UserRole) -> juniper::FieldResult<()> {
        match &self.role() {
            Some(user_role) if user_role == &required_role => Ok(()),
            _ => Err(juniper::FieldError::new(
                "Insufficient permissions",
                juniper::Value::null(),
            )),
        }
    }

    pub fn require_role_min(&self, required_role: UserRole) -> juniper::FieldResult<()> {
        match &self.role() {
            Some(user_role) if user_role >= &required_role => Ok(()),
            _ => Err(juniper::FieldError::new(
                "Insufficient permissions",
                juniper::Value::null(),
            )),
        }
    }

    pub fn require_authentication(&self) -> juniper::FieldResult<AuthenticatedUser> {
        if let Some(user) = &self.user {
            Ok(user.clone())
        } else {
            Err(juniper::FieldError::new(
                "Authentication required",
                juniper::Value::null(),
            ))
        }
    }

    pub fn get_ip(&self) -> &IpAddr {
        &self.ip
    }

    pub fn get_user_agent(&self) -> &str {
        &self.user_agent
    }

    pub fn get_signing_key(&self) -> &ed25519_dalek::SigningKey {
        &self.base.keypair
    }
}

pub type Schema = juniper::RootNode<Query, Mutation, EmptySubscription<Context>>;
