use anyhow::Context as AnyhowContext;
use mascord::commands::{admin, chat, mcp, memory, music, rag, settings};
use mascord::{config::Config, Data};
use poise::serenity_prelude as serenity;
use serenity::all::Http;
use songbird::serenity::SerenityInit;
use std::collections::HashSet;
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging with EnvFilter
    // Default: debug for mascord, info for key deps, warn for noisy HTTP internals
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
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
             rustls=warn",
        )
    });

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
            info!(
                "Using configured application ID ({}) and owner ID ({:?})",
                config.application_id, config.owner_id
            );
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
                info!(
                    "Fetched dynamic application ID: {} and owner: {:?}",
                    id, owner_id
                );
                (id, owner_id)
            }
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
                memory::memory(),
                rag::search(),
                music::join(),
                music::play(),
                music::skip(),
                music::leave(),
                music::queue(),
                admin::shutdown(),
                admin::restart(),
                mcp::mcp(),
                settings::settings(), // /settings context
            ],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    if let serenity::FullEvent::Message { new_message } = event {
                        if !new_message.author.bot {
                            // Check if channel tracking is enabled
                            let cache_message = new_message.clone();
                            let channel_id = new_message.channel_id.to_string();
                            let guild_id = new_message
                                .guild_id
                                .map(|id| id.to_string())
                                .unwrap_or_default();
                            let user_id = new_message.author.id.to_string();
                            let content = new_message.content.clone();
                            let message_id = new_message.id.to_string();
                            let timestamp = new_message.timestamp.unix_timestamp();

                            match data
                                .db
                                .run_blocking(move |db| {
                                    let enabled = db.is_channel_tracking_enabled(&channel_id)?;
                                    if enabled {
                                        db.save_message(
                                            &message_id,
                                            &guild_id,
                                            &channel_id,
                                            &user_id,
                                            &content,
                                            timestamp,
                                        )?;
                                    }
                                    Ok(enabled)
                                })
                                .await
                            {
                                Ok(true) => {
                                    // Populate internal cache after persistence check
                                    data.cache.insert(cache_message);
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to persist message {} in channel {}: {}",
                                        new_message.id,
                                        new_message.channel_id,
                                        e
                                    );
                                }
                            }

                            // Trigger chat via reply-to-bot or direct mention/tag.
                            let is_reply_to_bot = new_message
                                .referenced_message
                                .as_deref()
                                .is_some_and(|referenced| referenced.author.id.get() == data.bot_id);
                            if is_reply_to_bot {
                                if let Err(e) =
                                    mascord::reply::handle_reply(ctx, new_message, data).await
                                {
                                    tracing::error!("Error handling reply: {}", e);
                                }
                            } else {
                                let mentions_bot = new_message
                                    .mentions
                                    .iter()
                                    .any(|u| u.id.get() == data.bot_id);
                                if mentions_bot {
                                    if let Err(e) =
                                        mascord::mention::handle_mention(ctx, new_message, data)
                                            .await
                                    {
                                        tracing::error!("Error handling mention: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Ok(())
                })
            },
            on_error: |error| {
                Box::pin(async move {
                    match error {
                        poise::FrameworkError::Command { error, ctx, .. } => {
                            tracing::error!(
                                "Command error in {}: {}",
                                ctx.command().qualified_name,
                                error
                            );
                            let _ = ctx.send(
                                poise::CreateReply::default()
                                    .content(format!("❌ {}", error))
                                    .ephemeral(true)
                            ).await;
                        }
                        other => {
                            let _ = poise::builtins::on_error(other).await;
                        }
                    }
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
                let db = mascord::db::Database::new(&config).context("Failed to open database")?;
                db.execute_init().context("Failed to initialize database")?;

                // Initialize cache with capacity of 1000 messages
                let cache = mascord::cache::MessageCache::new(1000);

                // Initialize Tools
                let mut registry = mascord::tools::ToolRegistry::new();
                registry.register(std::sync::Arc::new(mascord::tools::builtin::music::PlayMusicTool));
                registry.register(std::sync::Arc::new(mascord::tools::builtin::rag::SearchLocalHistoryTool {
                    db: db.clone(),
                    llm: llm_client.clone(),
                }));
                registry.register(std::sync::Arc::new(
                    mascord::tools::builtin::user_memory::GetUserMemoryTool { db: db.clone() },
                ));
                let tools = std::sync::Arc::new(registry);

                // Initialize MCP
                let mcp_manager = std::sync::Arc::new(
                    mascord::mcp::client::McpClientManager::new(&config)
                        .context("Failed to initialize MCP manager")?
                );

                // Connect to MCP servers (best-effort) and warm up tool discovery.
                let mcp_timeout = tokio::time::Duration::from_secs(config.mcp_timeout_secs);
                let mut mcp_connect_handles = Vec::new();
                for mcp_config in config.mcp_servers.clone() {
                    let manager = mcp_manager.clone();
                    mcp_connect_handles.push(tokio::spawn(async move {
                        let name = mcp_config.name.clone();
                        let result = tokio::time::timeout(mcp_timeout, manager.connect(&mcp_config)).await;
                        (name, result)
                    }));
                }

                if !mcp_connect_handles.is_empty() {
                    let warmup_manager = mcp_manager.clone();
                    tokio::spawn(async move {
                        for handle in mcp_connect_handles {
                            match handle.await {
                                Ok((name, Ok(Ok(())))) => {
                                    tracing::info!("MCP server '{}' connected", name);
                                }
                                Ok((name, Ok(Err(e)))) => {
                                    tracing::error!(
                                        "Failed to connect to MCP server '{}': {}",
                                        name,
                                        e
                                    );
                                }
                                Ok((name, Err(_))) => {
                                    tracing::error!(
                                        "Timed out connecting to MCP server '{}' after {:?}",
                                        name,
                                        mcp_timeout
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("MCP connect task join error: {}", e);
                                }
                            }
                        }

                        let active = warmup_manager.list_active_servers().await;
                        let tools = warmup_manager.list_all_tools().await;
                        tracing::info!(
                            "MCP warmup: {} active servers, {} tools available",
                            active.len(),
                            tools.len()
                        );
                        if !tools.is_empty() {
                            let names = tools
                                .iter()
                                .map(|t| t.name().to_string())
                                .collect::<Vec<_>>()
                                .join(", ");
                            tracing::debug!("MCP tools: {}", names);
                        }
                    });
                }

                if config.summarization_enabled {
                    // Start background summarization task (tick interval configurable; triggers decide per-channel work)
                    let db_clone = db.clone();
                    let llm_clone = llm_client.clone();
                    let config_clone = config.clone();
                    tokio::spawn(async move {
                        let manager = mascord::summarize::SummarizationManager::new(
                            db_clone,
                            llm_clone,
                            &config_clone,
                        );
                        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
                            config_clone.summarization_interval_secs,
                        ));

                        loop {
                            interval.tick().await;
                            info!("Starting periodic background summarization cycle...");
                            match manager.get_active_channels().await {
                                Ok(channels) => {
                                    for channel_id in channels {
                                        match manager.should_summarize_channel(&channel_id).await {
                                            Ok(true) => {
                                                if let Err(e) =
                                                    manager.summarize_channel(&channel_id, 1).await
                                                {
                                                    tracing::error!(
                                                        "Failed to summarize channel {}: {}",
                                                        channel_id,
                                                        e
                                                    );
                                                }
                                            }
                                            Ok(false) => {}
                                            Err(e) => {
                                                tracing::error!(
                                                    "Failed to evaluate summarization trigger for channel {}: {}",
                                                    channel_id,
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Failed to fetch active channels: {}", e);
                                }
                            }
                        }
                    });
                }

                // Start YouTube cleanup task
                let download_dir = config.youtube_download_dir.clone();
                let cleanup_secs = config.youtube_cleanup_after_secs;
                tokio::spawn(async move {
                    mascord::voice::cleanup::start_cleanup_task(download_dir, cleanup_secs).await;
                });

                // Start short-term cache cleanup task (runs every hour).
                // This only prunes the in-memory cache, not the long-term RAG store.
                let cache_cleanup = cache.clone();
                let retention_hours = config.context_retention_hours;
                if retention_hours > 0 {
                    tokio::spawn(async move {
                        let mut interval =
                            tokio::time::interval(tokio::time::Duration::from_secs(3600));
                        loop {
                            interval.tick().await;
                            let removed = cache_cleanup.cleanup_old_messages(retention_hours);
                            if removed > 0 {
                                info!(
                                    "Short-term cache cleanup: removed {} messages (retention: {}h)",
                                    removed, retention_hours
                                );
                            }
                        }
                    });
                } else {
                    info!("Short-term cache cleanup disabled (CONTEXT_RETENTION_HOURS=0)");
                }

                // Start long-term retention cleanup task (runs every hour).
                // This applies to the RAG store (messages table).
                let db_cleanup = db.clone();
                let retention_days = config.long_term_retention_days;
                if retention_days > 0 {
                    tokio::spawn(async move {
                        let mut interval =
                            tokio::time::interval(tokio::time::Duration::from_secs(3600));
                        loop {
                            interval.tick().await;
                            let retention_hours = retention_days.saturating_mul(24);
                            match db_cleanup.cleanup_old_messages(retention_hours) {
                                Ok(count) if count > 0 => {
                                    info!(
                                        "Long-term cleanup: deleted {} old messages (retention: {} days)",
                                        count, retention_days
                                    );
                                }
                                Ok(_) => {
                                    tracing::debug!(
                                        "Long-term cleanup: no old messages to delete"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("Long-term cleanup error: {}", e);
                                }
                            }
                        }
                    });
                } else {
                    info!("Long-term cleanup disabled (LONG_TERM_RETENTION_DAYS=0)");
                }

                // Start user memory expiry cleanup (runs every hour).
                let user_memory_cleanup = db.clone();
                tokio::spawn(async move {
                    let mut interval =
                        tokio::time::interval(tokio::time::Duration::from_secs(3600));
                    loop {
                        interval.tick().await;
                        match user_memory_cleanup
                            .run_blocking(move |db| db.cleanup_expired_user_memory())
                            .await
                        {
                            Ok(count) if count > 0 => {
                                info!("User memory cleanup: deleted {} expired records", count);
                            }
                            Ok(_) => {
                                tracing::debug!("User memory cleanup: no expired records");
                            }
                            Err(e) => {
                                tracing::error!("User memory cleanup error: {}", e);
                            }
                        }
                    }
                });

                if config.embedding_indexer_enabled {
                    // Start background embedding indexer (best-effort, non-blocking).
                    // This avoids embedding calls on the Discord event handler hot path.
                    let db_index = db.clone();
                    let llm_index = std::sync::Arc::new(llm_client.clone());
                    let batch_size = config.embedding_indexer_batch_size;
                    let interval_secs = config.embedding_indexer_interval_secs;
                    tokio::spawn(async move {
                        mascord::indexer::EmbeddingIndexer::new(
                            db_index,
                            llm_index,
                            batch_size,
                            tokio::time::Duration::from_secs(interval_secs),
                        )
                        .run()
                        .await;
                    });
                }

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
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Could not register ctrl+c handler: {}", e);
            return;
        }
        info!("Received shutdown signal, closing shards...");
        shard_manager.shutdown_all().await;
    });

    info!("Bot is connecting to Discord...");
    if let Err(why) = client.start().await {
        error!("Fatal client error: {:?}", why);
    }

    Ok(())
}
