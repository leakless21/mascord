use mascord::{config::Config, Data};
use mascord::commands::{chat, rag, music, admin, mcp, settings};
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
                admin::shutdown(),
                mcp::mcp(),
                settings::settings(), // /settings context
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

                // Initialize Tools
                let mut registry = mascord::tools::ToolRegistry::new();
                registry.register(std::sync::Arc::new(mascord::tools::builtin::music::PlayMusicTool));
                registry.register(std::sync::Arc::new(mascord::tools::builtin::rag::SearchLocalHistoryTool { 
                    db: db.clone(),
                    llm: llm_client.clone(),
                }));
                registry.register(std::sync::Arc::new(mascord::tools::builtin::admin::ShutdownTool));
                let tools = std::sync::Arc::new(registry);

                // Initialize MCP
                let mcp_manager = std::sync::Arc::new(
                    mascord::mcp::client::McpClientManager::new(&config).expect("Failed to initialize MCP manager")
                );
                
                // Connect to MCP servers and discover tools
                for mcp_config in &config.mcp_servers {
                    let manager = mcp_manager.clone();
                    let mcp_config = mcp_config.clone();
                    tokio::spawn(async move {
                        if let Err(e) = manager.connect(&mcp_config).await {
                            tracing::error!("Failed to connect to MCP server {}: {}", mcp_config.name, e);
                        }
                    });
                }
                
                // Start background summarization task (runs every 4 hours)
                let db_clone = db.clone();
                let llm_clone = llm_client.clone();
                tokio::spawn(async move {
                    let _manager = mascord::summarize::SummarizationManager::new(db_clone, llm_clone);
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(4 * 3600));
                    
                    loop {
                        interval.tick().await;
                        // For now, we summarize the current channel history or all active channels
                        // A more advanced version would query unique channel_ids from the DB
                        info!("Triggering periodic background summarization...");
                        // TODO: Implement multi-channel discovery for summarization
                        // For MVP, we've enabled manual trigger via /settings context summarize
                    }
                });
                
                let bot_id = config.application_id;

                Ok(Data {
                    config,
                    http_client: reqwest::Client::new(),
                    llm_client,
                    db,
                    cache,
                    tools,
                    mcp_manager,
                    bot_id,
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
