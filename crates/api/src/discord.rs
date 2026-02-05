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

pub async fn remind_xtea() -> serenity::Result<()> {
    use serenity::all::{Builder, ChannelId, CreateMessage, GuildId};

    loop {
        if let Some(client) = get_client().await {
            if let Some(discord_public_invalid_submissions_channel) =
                std::env::var("DISCORD_PUBLIC_INVALID_SUBMISSIONS_CHANNEL_ID")
                    .ok()
                    .and_then(|id| id.parse::<u64>().ok())
                && let Some(discord_public_invalid_submissions_guild) =
                    std::env::var("DISCORD_PUBLIC_INVALID_SUBMISSIONS_GUILD_ID")
                        .ok()
                        .and_then(|id| id.parse::<u64>().ok())
            {
                Builder::execute(
                    CreateMessage::new().content("@xtea418 Still hasn't submitted the Google CTF application. Plz submit it now!"),
                    &client.http,
                    (ChannelId::new(discord_public_invalid_submissions_channel), Some(GuildId::new(discord_public_invalid_submissions_guild)))
                )
                .await?;
            }
        }
        tokio::time::sleep(std::time::Duration::from_hours(1)).await;
    }
}
