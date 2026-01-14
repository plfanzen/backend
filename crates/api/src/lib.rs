pub mod db;
pub mod graphql;
pub mod discord;

pub mod manager_api {
    tonic::include_proto!("plfanzen_ctf");
}
