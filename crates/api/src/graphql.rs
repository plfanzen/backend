// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::IpAddr;

use juniper::EmptySubscription;
pub use mutation::Mutation;
pub use query::Query;

use crate::db::models::UserRole;

pub mod auth;
mod handlers;
mod mutation;
mod query;

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
    user_id: Option<uuid::Uuid>,
    role: Option<UserRole>,
}

impl juniper::Context for Context {}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: uuid::Uuid,
    pub role: UserRole,
    pub actor: String,
    pub team_id: Option<uuid::Uuid>,
}

impl Context {
    pub fn new(
        base: BaseContext,
        ip: IpAddr,
        user_agent: String,
        user_details: Option<(uuid::Uuid, UserRole)>,
    ) -> Self {
        Self {
            base,
            ip,
            user_agent,
            user_id: user_details.as_ref().map(|(uid, _)| uid.clone()),
            role: user_details.map(|(_, role)| role),
        }
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
        self.user_id.is_some()
    }

    pub fn role(&self) -> Option<UserRole> {
        self.role
    }

    pub fn require_role_exact(&self, required_role: UserRole) -> juniper::FieldResult<()> {
        match &self.role {
            Some(user_role) if user_role == &required_role => Ok(()),
            _ => Err(juniper::FieldError::new(
                "Insufficient permissions",
                juniper::Value::null(),
            )),
        }
    }

    pub fn require_role_min(&self, required_role: UserRole) -> juniper::FieldResult<()> {
        match &self.role {
            Some(user_role) if user_role >= &required_role => Ok(()),
            _ => Err(juniper::FieldError::new(
                "Insufficient permissions",
                juniper::Value::null(),
            )),
        }
    }

    pub fn require_authentication(&self) -> juniper::FieldResult<AuthenticatedUser> {
        if let Some(uid) = self.user_id && let Some(role) = self.role {
            Ok(AuthenticatedUser {
                user_id: uid,
                role,
                actor: "todo".to_string(),
                team_id: None,
            })
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
