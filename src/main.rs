use mascord::{config::Config, Data};
use mascord::commands::{chat, rag, music, admin, mcp, settings};
use poise::serenity_prelude as serenity;
use tracing::{info, debug, error};
use tracing_subscriber::{prelude::*, EnvFilter, fmt};
use songbird::serenity::SerenityInit;
use serenity::all::Http;
use std::collections::HashSet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging with EnvFilter
    // Default: debug for mascord, info for key deps, warn for noisy HTTP internals
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(
            "mascord=debug,\
             poise=debug,\
             serenity=debug,\
             songbird=info,\
             reqwest=info,\
             async_openai=info,\
             rusqlite=info,\
             rmcp=info,\
             h2=warn,\
             hyper=warn,\
             hyper_util=warn,\
             rustls=warn"
        ));
    
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true).compact())
        .init();

    info!("Starting Mascord...");
    
    // Load configuration
    debug!("Loading configuration...");
    let mut config = Config::from_env()?;
    info!("Configuration loaded successfully");

    // Fetch dynamic application info (ID and Owners) only if APPLICATION_ID is missing
    let (app_id, owner_id) = if config.application_id != 0 {
        if config.owner_id.is_none() {
            tracing::warn!("OWNER_ID not set in config. Admin commands may not work. Skipping dynamic fetch to avoid rate limits.");
        } else {
            info!("Using configured application ID ({}) and owner ID ({:?})", config.application_id, config.owner_id);
        }
        (config.application_id, config.owner_id)
    } else {
        info!("Fetching dynamic application info from Discord...");
        let http = Http::new(&config.discord_token);
        match http.get_current_application_info().await {
            Ok(info) => {
                let mut owners = HashSet::new();
                let owner_id = if let Some(team) = info.team {
                    owners.insert(team.owner_user_id.get());
                    Some(team.owner_user_id.get())
                } else if let Some(owner) = &info.owner {
                    owners.insert(owner.id.get());
                    Some(owner.id.get())
                } else {
                    None
                };
                
                let id = info.id.get();
                info!("Fetched dynamic application ID: {} and owner: {:?}", id, owner_id);
                (id, owner_id)
            },
            Err(e) => {
                error!("Failed to fetch application info: {}. Cloudflare/Discord rate limits might be active. Falling back to config values.", e);
                (config.application_id, config.owner_id)
            }
        }
    };

    // Update config with active values
    config.application_id = app_id;
    if config.owner_id.is_none() {
        config.owner_id = owner_id;
    }

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
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    match event {
                        serenity::FullEvent::Message { new_message } => {
                            if !new_message.author.bot {
                                // Check if this is a reply to the bot
                                if let Some(referenced) = &new_message.referenced_message {
                                    if referenced.author.id.get() == data.bot_id {
                                        if let Err(e) = mascord::reply::handle_reply(ctx, new_message, data).await {
                                            tracing::error!("Error handling reply: {}", e);
                                        }
                                    }
                                }

                                // Check if channel tracking is enabled
                                if let Ok(true) = data.db.is_channel_tracking_enabled(&new_message.channel_id.to_string()) {
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
                
                // Optimized command registration (Ref: GAP-017 optimization)
                if config.register_commands {
                    if let Some(guild_id) = config.dev_guild_id {
                        info!("Registering commands specifically to development guild: {}", guild_id);
                        poise::builtins::register_in_guild(
                            ctx, 
                            &framework.options().commands, 
                            serenity::GuildId::new(guild_id)
                        ).await?;
                    } else {
                        info!("Registering commands globally (this can take up to an hour to propagate)...");
                        poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    }
                } else {
                    info!("Skipping command registration (REGISTER_COMMANDS=false). Use existing registration.");
                }
                
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
                
                // Start YouTube cleanup task
                let download_dir = config.youtube_download_dir.clone();
                let cleanup_secs = config.youtube_cleanup_after_secs;
                tokio::spawn(async move {
                    mascord::voice::cleanup::start_cleanup_task(download_dir, cleanup_secs).await;
                });

                // Start database message cleanup task (runs every hour)
                let db_cleanup = db.clone();
                let retention_hours = config.context_retention_hours;
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
                    loop {
                        interval.tick().await;
                        match db_cleanup.cleanup_old_messages(retention_hours) {
                            Ok(count) if count > 0 => {
                                info!("Database cleanup: deleted {} old messages (retention: {}h)", count, retention_hours);
                            }
                            Ok(_) => {
                                tracing::debug!("Database cleanup: no old messages to delete");
                            }
                            Err(e) => {
                                tracing::error!("Database cleanup error: {}", e);
                            }
                        }
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
    debug!("Poise framework built successfully");

    let intents = serenity::GatewayIntents::non_privileged() 
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::GUILD_VOICE_STATES;
    debug!("Creating Discord client...");

    let mut client = serenity::ClientBuilder::new(&discord_token, intents)
        .application_id(serenity::ApplicationId::new(app_id))
        .framework(framework)
        .register_songbird()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create client: {}", e))?;
    info!("Discord client created successfully");

    // Graceful shutdown handler
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Could not register ctrl+c handler");
        info!("Received shutdown signal, closing shards...");
        shard_manager.shutdown_all().await;
    });

    info!("Bot is connecting to Discord...");
    if let Err(why) = client.start().await {
        error!("Fatal client error: {:?}", why);
    }

    Ok(())
}
