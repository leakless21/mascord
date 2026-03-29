use anyhow::Context as AnyhowContext;
use mascord::commands::{about, admin, memory, music, rag, reminder, settings};
use mascord::{config::Config, Data};
use poise::serenity_prelude as serenity;
use serenity::all::Http;
use songbird::serenity::SerenityInit;
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load `.env` before tracing so `RUST_LOG` (and anything else read from the environment
    // by filters) applies. `Config::from_env()` also calls dotenv for tests/other entrypoints.
    // A parse error stops loading the rest of the file (e.g. unquoted spaces in values); warn loudly.
    if let Err(e) = dotenvy::dotenv() {
        eprintln!("mascord: WARNING: could not load .env: {e}");
    }

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
    if config.health_port > 0 {
        info!(
            "HTTP health server will bind on 0.0.0.0:{} (/healthz, /readyz)",
            config.health_port
        );
    } else {
        info!("HTTP health server disabled (HEALTH_PORT unset or 0; Homepage siteMonitor needs a listening port)");
    }

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

    let readiness = Arc::new(AtomicBool::new(false));
    if config.health_port > 0 {
        let health_port = config.health_port;
        let readiness_for_health = readiness.clone();
        tokio::spawn(async move {
            if let Err(e) =
                mascord::health::run_health_server(health_port, readiness_for_health).await
            {
                error!("Health server failed: {}", e);
            }
        });
    }

    let discord_token = config.discord_token.clone();
    let readiness_for_setup = readiness.clone();
    let scheduler_instance_id = format!(
        "{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let scheduler_instance_for_setup = scheduler_instance_id.clone();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                about::about(),
                memory::memory(),
                rag::search(),
                music::join(),
                music::play(),
                music::skip(),
                music::leave(),
                music::queue(),
                music::pause(),
                music::resume(),
                music::volume(),
                music::now_playing_cmd(),
                music::loop_cmd(),
                music::clear(),
                music::shuffle(),
                music::remove(),
                music::move_track(),
                reminder::reminder(),
                admin::shutdown(),
                admin::restart(),
                settings::settings(), // /settings context
            ],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    if let serenity::FullEvent::VoiceStateUpdate { old, new } = event {
                        mascord::voice::follow::handle_voice_follow_move(
                            ctx,
                            data,
                            old.as_ref(),
                            new,
                        )
                        .await;
                        mascord::voice::alone::handle_voice_alone_disconnect(ctx, data, new).await;
                    }
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
                        poise::FrameworkError::ArgumentParse { error, ctx, .. } => {
                            let detail = error.to_string();
                            tracing::error!(
                                "Slash argument parse/deserialize failed for `{}`: {}",
                                ctx.command().qualified_name,
                                detail
                            );
                            let hint = if detail.contains("deserialize")
                                || detail.contains("missing")
                                || detail.contains("Required")
                            {
                                format!(
                                    "❌ Discord sent options this build doesn't recognize (`{}`).\n\
                                     **Fix:** set `REGISTER_COMMANDS=true` once (or run `./scripts/register-commands.sh`) so slash commands match the bot, then try again.",
                                    detail
                                )
                            } else {
                                format!("❌ {}", detail)
                            };
                            let _ = ctx
                                .send(poise::CreateReply::default().content(hint).ephemeral(true))
                                .await;
                        }
                        poise::FrameworkError::CommandStructureMismatch {
                            ctx,
                            description,
                            ..
                        } => {
                            tracing::error!(
                                "Slash command structure mismatch for `/{}`: {}",
                                ctx.command.name,
                                description
                            );
                            let _ = ctx
                                .send(
                                    poise::CreateReply::default()
                                        .content(format!(
                                            "❌ This slash command does not match Discord's cached definition ({description}).\n\
                                             **Fix:** ask the bot owner to re-run slash registration (e.g. `./scripts/register-commands.sh {}` from the mascord repo), then try again.\n\
                                             If you use a **bridge** or unusual client, use the official Discord app and fill the **query** option before sending.",
                                            ctx.interaction
                                                .guild_id
                                                .map(|g| g.get().to_string())
                                                .unwrap_or_else(|| "--global".into())
                                        ))
                                        .ephemeral(true),
                                )
                                .await;
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
                let job_leases_enabled = config.job_leases_enabled;
                let job_lease_ttl_secs = config.job_lease_ttl_secs;
                let scheduler_instance_id = scheduler_instance_for_setup.clone();

                // Optimized command registration (Ref: GAP-017 optimization)
                if config.register_commands {
                    if let Some(guild_id) = config.dev_guild_id {
                        info!("Registering commands specifically to development guild: {}", guild_id);
                        poise::builtins::register_in_guild(
                            ctx,
                            &framework.options().commands,
                            serenity::GuildId::new(guild_id)
                        ).await?;
                        // Discord shows both global and guild commands in `/`; if globals were
                        // registered earlier, users see every command twice. Guild registration
                        // replaces only the guild scope — clear globals so dev guild is single-source.
                        info!("Clearing global application commands (prevents duplicate slash entries alongside this guild).");
                        serenity::Command::set_global_commands(ctx, vec![]).await?;
                    } else {
                        info!("Registering commands globally (this can take up to an hour to propagate)...");
                        poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                        // After moving from guild-only to global registration, old guild commands
                        // would still duplicate in that server unless cleared.
                        if let Ok(s) = std::env::var("CLEAR_GUILD_SLASH_ID") {
                            match s.parse::<u64>() {
                                Ok(gid) => {
                                    info!(
                                        "Clearing guild {} slash commands (CLEAR_GUILD_SLASH_ID)",
                                        gid
                                    );
                                    serenity::GuildId::new(gid).set_commands(ctx, vec![]).await?;
                                }
                                Err(_) => {
                                    tracing::warn!(
                                        "CLEAR_GUILD_SLASH_ID is not a valid u64: {:?}",
                                        s
                                    );
                                }
                            }
                        }
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
                let http_client = reqwest::Client::new();

                // Initialize Tools
                let mut registry = mascord::tools::ToolRegistry::new();
                registry.register(std::sync::Arc::new(mascord::tools::builtin::music::MusicTool));
                registry.register(std::sync::Arc::new(
                    mascord::tools::builtin::reminder::ReminderTool,
                ));
                registry.register(std::sync::Arc::new(mascord::tools::builtin::rag::SearchLocalHistoryTool {
                    db: db.clone(),
                    llm: llm_client.clone(),
                }));
                registry.register(std::sync::Arc::new(
                    mascord::tools::builtin::user_memory::GetUserMemoryTool { db: db.clone() },
                ));
                registry.register(std::sync::Arc::new(mascord::tools::builtin::web::WebSearchTool {
                    http_client: http_client.clone(),
                    searxng_url: config.searxng_url.clone(),
                    timeout_secs: config.web_tool_timeout_secs,
                    default_limit: config.web_search_default_limit,
                }));
                registry.register(std::sync::Arc::new(mascord::tools::builtin::web::FetchUrlTool {
                    http_client: http_client.clone(),
                    timeout_secs: config.web_tool_timeout_secs,
                    max_chars: config.web_fetch_max_chars,
                    jina_reader_base: config.jina_reader_base.clone(),
                }));
                let tools = std::sync::Arc::new(registry);

                if config.summarization_enabled {
                    // Start background summarization task (tick interval configurable; triggers decide per-channel work)
                    let db_clone = db.clone();
                    let cache_summarize = cache.clone();
                    let llm_clone = llm_client.clone();
                    let config_clone = config.clone();
                    let scheduler_instance = scheduler_instance_id.clone();
                    tokio::spawn(async move {
                        let manager = mascord::summarize::SummarizationManager::new(
                            db_clone.clone(),
                            llm_clone,
                            &config_clone,
                        );
                        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
                            config_clone.summarization_interval_secs,
                        ));

                        loop {
                            interval.tick().await;
                            if job_leases_enabled {
                                let db_for_lease = db_clone.clone();
                                let owner = scheduler_instance.clone();
                                let acquired = db_for_lease
                                    .run_blocking(move |db| {
                                        db.try_acquire_job_lease(
                                            "summarization",
                                            &owner,
                                            job_lease_ttl_secs,
                                        )
                                    })
                                    .await
                                    .unwrap_or(false);
                                if !acquired {
                                    continue;
                                }
                            }
                            if config_clone.summarization_activity_min_messages > 0 {
                                let gate_h = config_clone.summarization_activity_gate_hours;
                                let min_m = config_clone.summarization_activity_min_messages;
                                let db_gate = db_clone.clone();
                                let cache_cnt = cache_summarize
                                    .count_messages_in_window_hours(gate_h);
                                match db_gate
                                    .run_blocking(move |db| db.count_messages_in_window_hours(gate_h))
                                    .await
                                {
                                    Ok(db_cnt) => {
                                        // Short-term cache + long-term store: max avoids double-counting the same message.
                                        let activity = db_cnt.max(cache_cnt);
                                        if activity < min_m {
                                            tracing::debug!(
                                                "Summarization cycle skipped: activity {} in last {}h (db {}, cache {}; min {})",
                                                activity,
                                                gate_h,
                                                db_cnt,
                                                cache_cnt,
                                                min_m
                                            );
                                            continue;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Summarization activity gate query failed: {}",
                                            e
                                        );
                                        continue;
                                    }
                                }
                            }
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

                if config.autodream_enabled {
                    info!(
                        "AutoDream enabled: interval {}s, min {}h between consolidations; channel summaries={}",
                        config.autodream_interval_secs,
                        config.autodream_min_hours,
                        config.autodream_channel_summaries
                    );
                    let db_ad = db.clone();
                    let cache_ad = cache.clone();
                    let llm_ad = llm_client.clone();
                    let config_ad = config.clone();
                    let scheduler_instance_ad = scheduler_instance_id.clone();
                    let job_leases_ad = job_leases_enabled;
                    tokio::spawn(async move {
                        let service = mascord::services::autodream::AutoDreamService::new(
                            db_ad.clone(),
                            cache_ad,
                            llm_ad,
                            &config_ad,
                        );
                        let mut interval = tokio::time::interval(
                            tokio::time::Duration::from_secs(config_ad.autodream_interval_secs),
                        );
                        loop {
                            interval.tick().await;
                            if job_leases_ad {
                                let db_for_lease = db_ad.clone();
                                let owner = scheduler_instance_ad.clone();
                                let acquired = db_for_lease
                                    .run_blocking(move |db| {
                                        db.try_acquire_job_lease(
                                            "autodream",
                                            &owner,
                                            job_lease_ttl_secs,
                                        )
                                    })
                                    .await
                                    .unwrap_or(false);
                                if !acquired {
                                    continue;
                                }
                            }
                            match service.run_cycle().await {
                                Ok(()) => info!("AutoDream cycle completed"),
                                Err(e) => tracing::error!("AutoDream cycle error: {}", e),
                            }
                        }
                    });
                } else {
                    info!("AutoDream disabled (AUTODREAM_ENABLED=false)");
                }

                // Start reminder dispatcher (polls for due reminders).
                let reminder_service = mascord::services::reminder::ReminderService::new(db.clone());
                let reminder_http = ctx.http.clone();
                let reminder_poll_secs = config.reminder_poll_interval_secs;
                let reminder_batch_size = config.reminder_batch_size;
                let scheduler_instance = scheduler_instance_id.clone();
                let lease_db = db.clone();
                tokio::spawn(async move {
                    let dispatcher = mascord::reminders::ReminderDispatcher::new(
                        reminder_service,
                        reminder_http,
                        reminder_poll_secs,
                        reminder_batch_size,
                    );
                    if !job_leases_enabled {
                        dispatcher.run().await;
                        return;
                    }

                    let mut ticker =
                        tokio::time::interval(tokio::time::Duration::from_secs(reminder_poll_secs));
                    loop {
                        ticker.tick().await;
                        let db_for_lease = lease_db.clone();
                        let owner = scheduler_instance.clone();
                        let acquired = db_for_lease
                            .run_blocking(move |db| {
                                db.try_acquire_job_lease("reminders", &owner, job_lease_ttl_secs)
                            })
                            .await
                            .unwrap_or(false);
                        if !acquired {
                            continue;
                        }
                        if let Err(e) = dispatcher.run_once().await {
                            tracing::error!("Reminder dispatch cycle failed: {}", e);
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
                    let scheduler_instance = scheduler_instance_id.clone();
                    let lease_db = db.clone();
                    tokio::spawn(async move {
                        let indexer = mascord::indexer::EmbeddingIndexer::new(
                            db_index,
                            llm_index,
                            batch_size,
                            tokio::time::Duration::from_secs(interval_secs),
                        );
                        if !job_leases_enabled {
                            indexer.run().await;
                            return;
                        }
                        let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(
                            interval_secs,
                        ));
                        loop {
                            ticker.tick().await;
                            let db_for_lease = lease_db.clone();
                            let owner = scheduler_instance.clone();
                            let acquired = db_for_lease
                                .run_blocking(move |db| {
                                    db.try_acquire_job_lease(
                                        "embedding_indexer",
                                        &owner,
                                        job_lease_ttl_secs,
                                    )
                                })
                                .await
                                .unwrap_or(false);
                            if !acquired {
                                continue;
                            }
                            match indexer.run_once().await {
                                Ok(0) => tracing::debug!("Embedding indexer: no messages to index"),
                                Ok(n) => tracing::info!("Embedding indexer: indexed {} messages", n),
                                Err(e) => tracing::error!("Embedding indexer error: {}", e),
                            }
                        }
                    });
                }

                let bot_id = config.application_id;
                let music = std::sync::Arc::new(mascord::commands::music::MusicState::new());
                readiness_for_setup.store(true, Ordering::SeqCst);

                Ok(Data {
                    config,
                    http_client,
                    llm_client,
                    db,
                    cache,
                    tools,
                    music,
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
