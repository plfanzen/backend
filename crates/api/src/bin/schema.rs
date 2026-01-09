use juniper::{EmptySubscription, RootNode};

use plfanzen_api::graphql::{Mutation, Query};

fn main() {
    let schema = RootNode::new(
        Query,
        Mutation,
        EmptySubscription::<plfanzen_api::graphql::Context>::new(),
    );

    let result = schema.as_sdl();

    std::fs::write("schema.gql", result).expect("Unable to write schema file");
}
