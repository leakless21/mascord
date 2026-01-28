use mascord::{config::Config, Data};
use mascord::commands::{chat, rag, music};
use poise::serenity_prelude as serenity;
use tracing::{info, error};
use songbird::serenity::SerenityInit;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::from_env()?;
    let discord_token = config.discord_token.clone();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                chat::chat(),
                rag::search(),
                music::join(),
                music::play(),
                music::skip(),
                music::leave(),
                music::queue(),
            ],
            event_handler: |_ctx, event, _framework, data| {
                Box::pin(async move {
                    match event {
                        serenity::FullEvent::Message { new_message } => {
                            if !new_message.author.bot {
                                // Populate internal cache
                                data.cache.insert(new_message.clone());

                                let _ = data.db.save_message(
                                    &new_message.id.to_string(),
                                    &new_message.guild_id.map(|id| id.to_string()).unwrap_or_default(),
                                    &new_message.channel_id.to_string(),
                                    &new_message.author.id.to_string(),
                                    &new_message.content,
                                    new_message.timestamp.unix_timestamp(),
                                );
                            }
                        }
                        _ => {}
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                info!("Bot is ready!");
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                
                // Set bot status
                ctx.set_activity(Some(serenity::ActivityData::custom(&config.status_message)));
                
                let llm_client = mascord::llm::LlmClient::new(&config);
                let db = mascord::db::Database::new(&config).expect("Failed to open database");
                db.execute_init().expect("Failed to initialize database");
                
                // Initialize cache with capacity of 1000 messages
                let cache = mascord::cache::MessageCache::new(1000);

                Ok(Data {
                    config,
                    http_client: reqwest::Client::new(),
                    llm_client,
                    db,
                    cache,
                })
            })
        })
        .build();

    let intents = serenity::GatewayIntents::non_privileged() 
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let mut client = serenity::ClientBuilder::new(&discord_token, intents)
        .framework(framework)
        .register_songbird()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create client: {}", e))?;

    info!("Starting bot...");
    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }

    Ok(())
}
