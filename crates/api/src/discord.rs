use std::env;

use serenity::prelude::*;

use tokio::sync::OnceCell;

static DISCORD_CLIENT: OnceCell<Client> = OnceCell::const_new();

pub async fn get_client() -> Option<&'static Client> {
    if std::env::var("DISCORD_TOKEN").is_err() {
        return None;
    }
    Some(
        DISCORD_CLIENT
            .get_or_init(|| async {
                let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
                // Set gateway intents, which decides what events the bot will be notified about
                let intents = GatewayIntents::empty();

                Client::builder(&token, intents)
                    .await
                    .expect("Err creating client")
            })
            .await,
    )
}

pub async fn run_new_client() -> serenity::Result<()> {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::empty();

    let mut client = Client::builder(&token, intents)
        .await
        .expect("Err creating client");
    client.start().await
}
