use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::graphql::Actor;

pub trait HasOwnerUserId {
    fn user_id(&self) -> uuid::Uuid;
}

pub trait HasActor {
    async fn actor(&self, ctx: &crate::graphql::Context) -> juniper::FieldResult<Actor>;
}

impl<T: HasOwnerUserId> HasActor for T {
    async fn actor(&self, ctx: &crate::graphql::Context) -> juniper::FieldResult<Actor> {
        use crate::db::schema::teams;
        use crate::db::schema::users::dsl::*;

        let result = users
            .left_join(teams::table)
            .filter(id.eq(self.user_id()))
            .select((username, teams::id.nullable(), teams::slug.nullable()))
            .first::<(String, Option<uuid::Uuid>, Option<String>)>(&mut ctx.get_db_conn().await)
            .await?;

        match result.1 {
            Some(team_id_2) => Ok(Actor::Team {
                id: team_id_2,
                slug: result.2.unwrap_or_default(),
            }),
            None => Ok(Actor::User {
                id: self.user_id(),
                username: result.0,
            }),
        }
    }
}
