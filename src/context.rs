//! Conversation context management for persistent LLM memory
//!
//! Provides per-channel context retrieval for injecting recent message history
//! into LLM conversations.

use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage as ReqMsg,
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
    ChatCompletionRequestUserMessageContent,
};
use chrono::{DateTime, Duration, Utc};
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;

use crate::cache::MessageCache;
use crate::config::Config;
use crate::db::{Database, StoredChannelMessage};
use tracing::{debug, warn};

/// Formats cached messages into LLM-compatible context messages
pub struct ConversationContext;

impl ConversationContext {
    /// Async wrapper around `get_context_for_channel` to offload DB work from the async runtime.
    pub async fn get_context_for_channel_async(
        cache: MessageCache,
        db: Database,
        config: Config,
        channel_id: ChannelId,
        guild_id: Option<u64>,
        bot_id: Option<u64>,
        exclude_message_id: Option<u64>,
    ) -> Vec<ChatCompletionRequestMessage> {
        tokio::task::spawn_blocking(move || {
            Self::get_context_for_channel(
                &cache,
                &db,
                &config,
                channel_id,
                guild_id,
                bot_id,
                exclude_message_id,
            )
        })
        .await
        .unwrap_or_else(|e| {
            warn!(
                "Context: Failed to fetch context for channel {}: {}",
                channel_id, e
            );
            Vec::new()
        })
    }

    /// Retrieves recent channel messages and formats them for LLM context
    ///
    /// Messages are filtered by:
    /// - Channel ID
    /// - Retention period (config.context_retention_hours)
    /// - Limit (config.context_message_limit)
    ///
    /// Returns messages oldest-first, formatted as user/assistant messages
    pub fn get_context_for_channel(
        cache: &MessageCache,
        db: &Database,
        config: &Config,
        channel_id: ChannelId,
        guild_id: Option<u64>,
        bot_id: Option<u64>,
        exclude_message_id: Option<u64>,
    ) -> Vec<ChatCompletionRequestMessage> {
        // Resolve settings: Check DB -> Fallback to Config
        let (limit, retention) = if let Some(gid) = guild_id {
            match db.get_guild_settings(gid) {
                Ok(settings) => settings,
                Err(e) => {
                    warn!("Context: Failed to load guild settings for {}: {}", gid, e);
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        let limit = limit.unwrap_or(config.context_message_limit);
        let retention = retention.unwrap_or(config.context_retention_hours);

        let mut cutoff_unix = if retention == 0 {
            None
        } else {
            Some((Utc::now() - Duration::hours(retention as i64)).timestamp())
        };

        // 0. Check Channel Specific Settings (Enabled + Scope)
        match db.get_channel_settings(&channel_id.to_string()) {
            Ok(Some((enabled, scope_date))) => {
                if !enabled {
                    debug!(
                        "Context: Channel {} memory disabled; skipping context",
                        channel_id
                    );
                    return Vec::new();
                }

                if let Some(scope_date) = scope_date {
                    // If we have a memory start date, use it if it's MORE RECENT than the retention cutoff
                    let scope_date_clone = scope_date.clone();
                    if let Ok(scope_ts) = chrono::DateTime::parse_from_str(
                        &format!("{} +0000", scope_date),
                        "%Y-%m-%d %H:%M:%S %z",
                    ) {
                        let scope_unix = scope_ts.timestamp();
                        let should_update = cutoff_unix.is_none_or(|cutoff| scope_unix > cutoff);
                        if should_update {
                            debug!(
                                "Context: Respecting memory scope for channel {}: messages after {}",
                                channel_id, scope_date_clone
                            );
                            cutoff_unix = Some(scope_unix);
                        }
                    } else {
                        warn!(
                            "Context: Invalid memory scope date '{}' for channel {}",
                            scope_date_clone, channel_id
                        );
                    }
                }
            }
            Ok(_) => {}
            Err(e) => warn!(
                "Context: Failed to load channel settings for {}: {}",
                channel_id, e
            ),
        }

        let mut messages = Vec::new();

        // 1. Inject Working Memory (Latest Summary) if available
        match db.get_latest_summary(&channel_id.to_string()) {
            Ok(Some(summary)) => {
                debug!(
                    "Context: Injecting working memory (summary) for channel {}",
                    channel_id
                );
                use async_openai::types::ChatCompletionRequestSystemMessageArgs;
                if let Ok(msg) = ChatCompletionRequestSystemMessageArgs::default()
                    .content(format!(
                        "Earlier conversation summary for this channel:\n{}",
                        summary
                    ))
                    .build()
                {
                    messages.push(msg.into());
                }
            }
            Ok(None) => {}
            Err(e) => warn!(
                "Context: Failed to load summary for channel {}: {}",
                channel_id, e
            ),
        }

        // 2. Fetch Short-Term context (verbatim messages)
        if retention == 0 {
            debug!(
                "Context: Fetching up to {} messages for channel {} (retention: disabled)",
                limit, channel_id
            );
        } else {
            debug!(
                "Context: Fetching up to {} messages for channel {} (retention: {}h)",
                limit, channel_id, retention
            );
        }
        let entries = cache.get_channel_history(channel_id, limit);

        let mut short_term_messages: Vec<ChatCompletionRequestMessage> = entries
            .into_iter()
            .filter(|msg| exclude_message_id.is_none_or(|exclude| msg.id.get() != exclude))
            .filter(|msg| {
                // Filter by retention period using unix timestamps (unless disabled)
                cutoff_unix.is_none_or(|cutoff| msg.timestamp.unix_timestamp() > cutoff)
            })
            .filter_map(|msg| Self::format_message(&msg, bot_id))
            .collect();

        let n_from_cache = short_term_messages.len();

        // In-memory cache is empty after restarts and only fills as new messages arrive; persist
        // the same rows in SQLite, so we can still show recent history to the LLM.
        if short_term_messages.is_empty() {
            let since = match cutoff_unix {
                Some(ts) => DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now),
                None => DateTime::from_timestamp(0, 0).expect("epoch"),
            };
            match db.get_recent_messages_for_channel_context(
                &channel_id.to_string(),
                since,
                limit,
            ) {
                Ok(mut rows) => {
                    rows.reverse();
                    for row in rows {
                        if exclude_message_id.is_some_and(|ex| row.discord_id == ex.to_string()) {
                            continue;
                        }
                        if let Some(m) = Self::format_stored_message(&row, bot_id) {
                            short_term_messages.push(m);
                        }
                    }
                    if !short_term_messages.is_empty() {
                        debug!(
                            "Context: Hydrated {} short-term messages from DB (cache had 0)",
                            short_term_messages.len()
                        );
                    }
                }
                Err(e) => warn!(
                    "Context: Failed to load recent messages from DB for channel {}: {}",
                    channel_id, e
                ),
            }
        }

        debug!(
            "Context: Retrieved {} short-term messages for channel {} ({} from in-memory cache)",
            short_term_messages.len(),
            channel_id,
            n_from_cache
        );
        short_term_messages = Self::apply_context_hygiene(short_term_messages);
        messages.append(&mut short_term_messages);
        messages
    }

    /// Collapse consecutive duplicate user/assistant text turns to cut repeated spam in busy channels.
    fn apply_context_hygiene(messages: Vec<ChatCompletionRequestMessage>) -> Vec<ChatCompletionRequestMessage> {
        if messages.len() < 2 {
            return messages;
        }
        let mut out: Vec<ChatCompletionRequestMessage> = Vec::with_capacity(messages.len());
        for m in messages {
            let dup = match (out.last(), Self::context_message_fingerprint(&m)) {
                (Some(prev), Some(fp)) => Self::context_message_fingerprint(prev).as_ref() == Some(&fp),
                _ => false,
            };
            if !dup {
                out.push(m);
            }
        }
        out
    }

    fn normalize_ws(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn context_message_fingerprint(m: &ChatCompletionRequestMessage) -> Option<String> {
        match m {
            ReqMsg::User(u) => match &u.content {
                ChatCompletionRequestUserMessageContent::Text(t) => {
                    Some(format!("u:{}", Self::normalize_ws(t)))
                }
                _ => None,
            },
            ReqMsg::Assistant(a) => match &a.content {
                Some(ChatCompletionRequestAssistantMessageContent::Text(t)) => {
                    Some(format!("a:{}", Self::normalize_ws(t)))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Formats a Discord message into an LLM message
    fn format_message(msg: &Message, bot_id: Option<u64>) -> Option<ChatCompletionRequestMessage> {
        // Skip empty messages
        if msg.content.trim().is_empty() {
            return None;
        }

        let is_bot = bot_id.is_some_and(|id| msg.author.id.get() == id);

        if is_bot {
            // Bot's own messages become assistant messages
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(msg.content.clone())
                .build()
                .ok()
                .map(|m| m.into())
        } else {
            // Other users' messages become user messages with attribution
            let formatted = format!("[{}]: {}", msg.author.name, msg.content);
            ChatCompletionRequestUserMessageArgs::default()
                .content(formatted)
                .build()
                .ok()
                .map(|m| m.into())
        }
    }

    fn format_stored_message(
        row: &StoredChannelMessage,
        bot_id: Option<u64>,
    ) -> Option<ChatCompletionRequestMessage> {
        if row.content.trim().is_empty() {
            return None;
        }
        let uid = row.user_id.parse::<u64>().ok();
        let is_bot = match (uid, bot_id) {
            (Some(u), Some(b)) => u == b,
            _ => false,
        };
        if is_bot {
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(row.content.clone())
                .build()
                .ok()
                .map(|m| m.into())
        } else {
            let label = format!("[user:{}]", row.user_id);
            ChatCompletionRequestUserMessageArgs::default()
                .content(format!("{}: {}", label, row.content))
                .build()
                .ok()
                .map(|m| m.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_openai::types::ChatCompletionRequestUserMessageArgs;
    use serenity::model::id::MessageId;
    use serenity::model::id::UserId;
    use serenity::model::timestamp::Timestamp;
    use serenity::model::user::User;

    fn mock_config() -> Config {
        Config {
            discord_token: "test".to_string(),
            application_id: 0,
            owner_id: None,
            llama_url: "test".to_string(),
            llama_model: "test".to_string(),
            llama_api_key: None,
            embedding_url: "test".to_string(),
            embedding_model: "test".to_string(),
            embedding_api_key: None,
            database_url: ":memory:".to_string(),
            system_prompt: "test".to_string(),
            max_context_messages: 10,
            status_message: "test".to_string(),
            youtube_cookies: None,
            youtube_download_dir: "/tmp".to_string(),
            youtube_cleanup_after_secs: 3600,
            lavalink_enabled: false,
            lavalink_host: "127.0.0.1:2333".to_string(),
            lavalink_password: String::new(),
            lavalink_search_prefix: "ytsearch".to_string(),
            lavalink_sponsorblock_categories: None,
            lavalink_use_lavasearch: false,
            searxng_url: "http://localhost:8086".to_string(),
            web_tool_timeout_secs: 20,
            web_search_default_limit: 5,
            web_fetch_max_chars: 8000,
            jina_reader_base: "https://r.jina.ai".to_string(),
            context_message_limit: 5,
            context_retention_hours: 24,
            llm_timeout_secs: 120,
            embedding_timeout_secs: 30,
            log_llm_requests: false,
            log_llm_responses: false,
            log_llm_tool_args: false,
            voice_idle_timeout_secs: 180,
            voice_alone_timeout_secs: 90,
            voice_follow_user_move: false,
            max_queue_tracks: 75,
            voice_allow_duplicate_urls: true,
            dev_guild_id: None,
            register_commands: false,
            agent_confirm_timeout_secs: 300,
            agent_run_timeout_secs: 180,
            embedding_indexer_enabled: true,
            embedding_indexer_batch_size: 25,
            embedding_indexer_interval_secs: 30,
            summarization_enabled: true,
            summarization_interval_secs: 3600,
            summarization_active_channels_lookback_days: 7,
            summarization_initial_min_messages: 50,
            summarization_trigger_new_messages: 150,
            summarization_trigger_age_hours: 6,
            summarization_trigger_min_new_messages: 20,
            summarization_max_tokens: 1200,
            summarization_refresh_weeks: 6,
            summarization_refresh_days_lookback: 14,
            summarization_activity_gate_hours: 48,
            summarization_activity_min_messages: 5,
            reminder_poll_interval_secs: 30,
            reminder_batch_size: 25,
            health_port: 0,
            job_leases_enabled: false,
            job_lease_ttl_secs: 120,
            long_term_retention_days: 365,
            autodream_enabled: true,
            autodream_interval_secs: 86400,
            autodream_min_hours: 24,
            autodream_max_users_per_cycle: 8,
            autodream_channel_summaries: true,
            autodream_max_channels_per_cycle: 4,
            autodream_user_max_chars: 1200,
            autodream_activity_gate_hours: 48,
            autodream_activity_min_messages: 5,
            autodream_channel_activity_hours: 72,
        }
    }

    fn mock_message(
        id: u64,
        channel_id: u64,
        user_id: u64,
        content: &str,
        username: &str,
    ) -> Message {
        let mut msg = Message::default();
        msg.id = MessageId::new(id);
        msg.channel_id = ChannelId::new(channel_id);
        msg.author = User::default();
        msg.author.id = UserId::new(user_id);
        msg.author.name = username.to_string();
        msg.content = content.to_string();
        msg.timestamp = Timestamp::now();
        msg
    }

    #[test]
    fn test_context_retrieval() {
        let cache = MessageCache::new(100);
        let config = mock_config();

        // Setup in-memory DB
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        cache.insert(mock_message(1, 100, 1, "Hello everyone", "Alice"));
        cache.insert(mock_message(2, 100, 2, "Hi Alice!", "Bob"));
        cache.insert(mock_message(
            3,
            100,
            999,
            "Hello, how can I help?",
            "Mascord",
        )); // Bot
        cache.insert(mock_message(4, 100, 1, "What's the weather?", "Alice"));

        let context = ConversationContext::get_context_for_channel(
            &cache,
            &db,
            &config,
            ChannelId::new(100),
            Some(123),
            Some(999), // Bot ID
            None,
        );

        assert_eq!(context.len(), 4);
    }

    #[test]
    fn test_context_limit() {
        let cache = MessageCache::new(100);
        let config = mock_config(); // limit = 5
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        for i in 1..=10 {
            cache.insert(mock_message(i, 100, 1, &format!("Message {}", i), "User"));
        }

        let context = ConversationContext::get_context_for_channel(
            &cache,
            &db,
            &config,
            ChannelId::new(100),
            Some(123),
            None,
            None,
        );

        // Should only get 5 messages (the most recent ones)
        assert_eq!(context.len(), 5);
    }

    #[test]
    fn test_context_empty_cache_falls_back_to_db() {
        let cache = MessageCache::new(100);
        let config = mock_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        let ts = Utc::now().timestamp();
        db.save_message("m1", "g1", "100", "1", "Play something", ts)
            .unwrap();

        let context = ConversationContext::get_context_for_channel(
            &cache,
            &db,
            &config,
            ChannelId::new(100),
            Some(123),
            None,
            None,
        );

        assert!(
            !context.is_empty(),
            "expected DB fallback when in-memory cache has no rows"
        );
    }

    #[test]
    fn test_context_retention_disabled_includes_old_messages() {
        let cache = MessageCache::new(100);
        let mut config = mock_config();
        config.context_retention_hours = 0;
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        let mut old_msg = mock_message(1, 100, 1, "Old", "User");
        old_msg.timestamp = Timestamp::from_unix_timestamp(1).unwrap();
        let mut new_msg = mock_message(2, 100, 1, "New", "User");
        new_msg.timestamp = Timestamp::from_unix_timestamp(Utc::now().timestamp()).unwrap();

        cache.insert(old_msg);
        cache.insert(new_msg);

        let context = ConversationContext::get_context_for_channel(
            &cache,
            &db,
            &config,
            ChannelId::new(100),
            Some(123),
            None,
            None,
        );

        assert_eq!(context.len(), 2);
    }

    #[test]
    fn test_apply_context_hygiene_dedupes_consecutive_identical() {
        let u1: ChatCompletionRequestMessage = ChatCompletionRequestUserMessageArgs::default()
            .content("same line")
            .build()
            .unwrap()
            .into();
        let u2: ChatCompletionRequestMessage = ChatCompletionRequestUserMessageArgs::default()
            .content("same line")
            .build()
            .unwrap()
            .into();
        let u3: ChatCompletionRequestMessage = ChatCompletionRequestUserMessageArgs::default()
            .content("other")
            .build()
            .unwrap()
            .into();
        let out = ConversationContext::apply_context_hygiene(vec![u1, u2, u3]);
        assert_eq!(out.len(), 2);
    }
}
