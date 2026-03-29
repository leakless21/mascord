use dotenvy::dotenv;
use serde::Deserialize;
use std::env;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub discord_token: String,
    pub application_id: u64,
    pub owner_id: Option<u64>,
    pub llama_url: String,
    pub llama_model: String,
    pub llama_api_key: Option<String>,
    pub embedding_url: String,
    pub embedding_model: String,
    pub embedding_api_key: Option<String>,
    pub database_url: String,
    pub system_prompt: String,
    pub max_context_messages: usize,
    pub status_message: String,
    pub youtube_cookies: Option<String>,
    pub youtube_download_dir: String,
    pub youtube_cleanup_after_secs: u64,
    pub searxng_url: String,
    pub web_tool_timeout_secs: u64,
    pub web_search_default_limit: usize,
    pub web_fetch_max_chars: usize,
    pub jina_reader_base: String,
    // Context persistence settings
    pub context_message_limit: usize,
    pub context_retention_hours: u64,
    // Timeout & Maintenance settings
    pub llm_timeout_secs: u64,
    pub embedding_timeout_secs: u64,
    /// Seconds after the queue is empty (last track ended) before leaving voice (`0` = leave immediately after drain).
    pub voice_idle_timeout_secs: u64,
    /// Seconds to wait after the voice channel has no human listeners before disconnecting (`0` = disabled).
    pub voice_alone_timeout_secs: u64,
    /// Move the bot when the last human listener switches to another VC (`false` = predictable; enable for “follow me”).
    pub voice_follow_user_move: bool,
    /// Max tracks in the session queue (current + waiting); `0` = no limit (capped at 500 when set).
    pub max_queue_tracks: usize,
    /// Allow enqueueing the same URL more than once.
    pub voice_allow_duplicate_urls: bool,
    pub dev_guild_id: Option<u64>,
    pub register_commands: bool,

    // Agent confirmation settings
    pub agent_confirm_timeout_secs: u64,

    // Background embedding indexer settings
    pub embedding_indexer_enabled: bool,
    pub embedding_indexer_batch_size: usize,
    pub embedding_indexer_interval_secs: u64,

    // Background summarization settings
    pub summarization_enabled: bool,
    pub summarization_interval_secs: u64,
    pub summarization_active_channels_lookback_days: i64,
    pub summarization_initial_min_messages: usize,
    pub summarization_trigger_new_messages: usize,
    pub summarization_trigger_age_hours: i64,
    pub summarization_trigger_min_new_messages: usize,
    pub summarization_max_tokens: usize,
    pub summarization_refresh_weeks: i64,
    pub summarization_refresh_days_lookback: i64,
    /// Skip each summarization tick if fewer than this many DB messages in the gate window (`0` = no gate).
    pub summarization_activity_gate_hours: u64,
    pub summarization_activity_min_messages: usize,

    // Reminder scheduler settings
    pub reminder_poll_interval_secs: u64,
    pub reminder_batch_size: usize,
    pub health_port: u16,
    pub job_leases_enabled: bool,
    pub job_lease_ttl_secs: u64,

    // Long-term retention (RAG store)
    pub long_term_retention_days: u64,

    /// Background memory consolidation (AutoDream): LLM pass over user + optional channel summaries.
    pub autodream_enabled: bool,
    pub autodream_interval_secs: u64,
    pub autodream_min_hours: i64,
    pub autodream_max_users_per_cycle: usize,
    pub autodream_channel_summaries: bool,
    pub autodream_max_channels_per_cycle: usize,
    pub autodream_user_max_chars: usize,
    /// Skip each AutoDream cycle if fewer than this many DB messages in the gate window (`0` = no gate).
    pub autodream_activity_gate_hours: u64,
    pub autodream_activity_min_messages: usize,
    /// Only consolidate channel summaries that had ≥1 message in this many hours (`0` = do not filter by channel activity).
    pub autodream_channel_activity_hours: u64,
}

/// `:memory:` SQLite and safe defaults for unit tests (`db`, reminders, tools).
#[cfg(test)]
pub(crate) fn test_memory_config() -> Config {
    Config {
        discord_token: "test".to_string(),
        application_id: 0,
        owner_id: Some(1),
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
        searxng_url: "http://localhost:8086".to_string(),
        web_tool_timeout_secs: 20,
        web_search_default_limit: 5,
        web_fetch_max_chars: 8000,
        jina_reader_base: "https://r.jina.ai".to_string(),
        context_message_limit: 50,
        context_retention_hours: 24,
        llm_timeout_secs: 120,
        embedding_timeout_secs: 30,
        // Voice: ~3 min after queue ends; ~1.5 min when VC has no humans; follow off by default.
        voice_idle_timeout_secs: 180,
        voice_alone_timeout_secs: 90,
        voice_follow_user_move: false,
        max_queue_tracks: 75,
        voice_allow_duplicate_urls: true,
        dev_guild_id: None,
        register_commands: false,
        agent_confirm_timeout_secs: 300,
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

/// Default when `SYSTEM_PROMPT` is unset.
/// Style follows common agent prompts: role + tone + *when* to use tools; tool definitions carry *which* tools and their parameters (avoid duplicating schemas here).
const DEFAULT_SYSTEM_PROMPT: &str = "You are Mascord, a Discord assistant. \
Be clear and concise; a little snark and dry wit are on-brand—clever, never cruel or punching down. \
When the user wants something you can do via the available tools, call the right tool and pass arguments that match its schema—use only names and parameters from the tool list, never invented tools. \
For questions, opinions, or chat that does not require an action, answer in plain text.";

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenv().ok();
        Self::build()
    }

    fn build() -> anyhow::Result<Self> {
        Ok(Config {
            discord_token: env::var("DISCORD_TOKEN")
                .map_err(|_| anyhow::anyhow!("DISCORD_TOKEN must be set"))?,
            application_id: env::var("APPLICATION_ID")
                .map_err(|_| anyhow::anyhow!("APPLICATION_ID must be set"))?
                .parse()
                .map_err(|_| anyhow::anyhow!("APPLICATION_ID must be a valid u64"))?,
            owner_id: env::var("OWNER_ID").ok().and_then(|id| id.parse().ok()),
            llama_url: env::var("LLAMA_URL")
                .unwrap_or_else(|_| "http://localhost:8080/v1".to_string()),
            llama_model: env::var("LLAMA_MODEL").unwrap_or_else(|_| "local-model".to_string()),
            llama_api_key: env::var("LLAMA_API_KEY").ok(),
            embedding_url: env::var("EMBEDDING_URL").unwrap_or_else(|_| {
                env::var("LLAMA_URL").unwrap_or_else(|_| "http://localhost:8080/v1".to_string())
            }),
            embedding_model: env::var("EMBEDDING_MODEL")
                .unwrap_or_else(|_| "local-model".to_string()),
            embedding_api_key: env::var("EMBEDDING_API_KEY").ok(),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "data/mascord.db".to_string()),
            system_prompt: env::var("SYSTEM_PROMPT")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string()),
            max_context_messages: env::var("MAX_CONTEXT_MESSAGES")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            status_message: env::var("STATUS_MESSAGE")
                .unwrap_or_else(|_| "Ready to assist!".to_string()),
            youtube_cookies: env::var("YOUTUBE_COOKIES").ok(),
            youtube_download_dir: env::var("YOUTUBE_DOWNLOAD_DIR")
                .unwrap_or_else(|_| "/tmp/mascord_audio".to_string()),
            youtube_cleanup_after_secs: env::var("YOUTUBE_CLEANUP_AFTER_SECS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .unwrap_or(3600),
            searxng_url: env::var("SEARXNG_URL")
                .unwrap_or_else(|_| "http://localhost:8086".to_string()),
            web_tool_timeout_secs: env::var("WEB_TOOL_TIMEOUT_SECS")
                .unwrap_or_else(|_| "20".to_string())
                .parse()
                .unwrap_or(20),
            web_search_default_limit: env::var("WEB_SEARCH_DEFAULT_LIMIT")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            web_fetch_max_chars: env::var("WEB_FETCH_MAX_CHARS")
                .unwrap_or_else(|_| "8000".to_string())
                .parse()
                .unwrap_or(8000),
            jina_reader_base: env::var("JINA_READER_BASE")
                .unwrap_or_else(|_| "https://r.jina.ai".to_string()),
            context_message_limit: env::var("CONTEXT_MESSAGE_LIMIT")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            context_retention_hours: env::var("CONTEXT_RETENTION_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
            llm_timeout_secs: env::var("LLM_TIMEOUT_SECS")
                .unwrap_or_else(|_| "120".to_string())
                .parse()
                .unwrap_or(120),
            embedding_timeout_secs: env::var("EMBEDDING_TIMEOUT_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            voice_idle_timeout_secs: env::var("VOICE_IDLE_TIMEOUT_SECS")
                .unwrap_or_else(|_| "180".to_string())
                .parse()
                .unwrap_or(180),
            voice_alone_timeout_secs: env::var("VOICE_ALONE_TIMEOUT_SECS")
                .unwrap_or_else(|_| "90".to_string())
                .parse()
                .unwrap_or(90),
            voice_follow_user_move: env::var("VOICE_FOLLOW_USER_MOVE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            max_queue_tracks: {
                let n: usize = env::var("MAX_QUEUE_TRACKS")
                    .unwrap_or_else(|_| "75".to_string())
                    .parse()
                    .unwrap_or(75);
                if n == 0 {
                    0
                } else {
                    n.min(500)
                }
            },
            voice_allow_duplicate_urls: env::var("VOICE_ALLOW_DUPLICATE_URLS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            dev_guild_id: env::var("DEV_GUILD_ID").ok().and_then(|id| id.parse().ok()),
            register_commands: env::var("REGISTER_COMMANDS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            agent_confirm_timeout_secs: env::var("AGENT_CONFIRM_TIMEOUT_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),

            embedding_indexer_enabled: env::var("EMBEDDING_INDEXER_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            embedding_indexer_batch_size: env::var("EMBEDDING_INDEXER_BATCH_SIZE")
                .unwrap_or_else(|_| "25".to_string())
                .parse()
                .unwrap_or(25),
            embedding_indexer_interval_secs: env::var("EMBEDDING_INDEXER_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),

            summarization_enabled: env::var("SUMMARIZATION_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            summarization_interval_secs: env::var("SUMMARIZATION_INTERVAL_SECS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .unwrap_or(3600),
            summarization_active_channels_lookback_days: env::var(
                "SUMMARIZATION_ACTIVE_CHANNELS_LOOKBACK_DAYS",
            )
            .unwrap_or_else(|_| "7".to_string())
            .parse()
            .unwrap_or(7),
            summarization_initial_min_messages: env::var("SUMMARIZATION_INITIAL_MIN_MESSAGES")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            summarization_trigger_new_messages: env::var("SUMMARIZATION_TRIGGER_NEW_MESSAGES")
                .unwrap_or_else(|_| "150".to_string())
                .parse()
                .unwrap_or(150),
            summarization_trigger_age_hours: env::var("SUMMARIZATION_TRIGGER_AGE_HOURS")
                .unwrap_or_else(|_| "6".to_string())
                .parse()
                .unwrap_or(6),
            summarization_trigger_min_new_messages: env::var(
                "SUMMARIZATION_TRIGGER_MIN_NEW_MESSAGES",
            )
            .unwrap_or_else(|_| "20".to_string())
            .parse()
            .unwrap_or(20),
            summarization_max_tokens: env::var("SUMMARIZATION_MAX_TOKENS")
                .unwrap_or_else(|_| "1200".to_string())
                .parse()
                .unwrap_or(1200),
            summarization_refresh_weeks: env::var("SUMMARIZATION_REFRESH_WEEKS")
                .unwrap_or_else(|_| "6".to_string())
                .parse()
                .unwrap_or(6),
            summarization_refresh_days_lookback: env::var("SUMMARIZATION_REFRESH_DAYS_LOOKBACK")
                .unwrap_or_else(|_| "14".to_string())
                .parse()
                .unwrap_or(14),
            summarization_activity_gate_hours: env::var("SUMMARIZATION_ACTIVITY_GATE_HOURS")
                .unwrap_or_else(|_| "48".to_string())
                .parse()
                .unwrap_or(48),
            summarization_activity_min_messages: env::var("SUMMARIZATION_ACTIVITY_MIN_MESSAGES")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            reminder_poll_interval_secs: env::var("REMINDER_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            reminder_batch_size: env::var("REMINDER_BATCH_SIZE")
                .unwrap_or_else(|_| "25".to_string())
                .parse()
                .unwrap_or(25),
            health_port: env::var("HEALTH_PORT")
                .unwrap_or_else(|_| "0".to_string())
                .trim()
                .parse()
                .unwrap_or(0),
            job_leases_enabled: env::var("JOB_LEASES_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            job_lease_ttl_secs: env::var("JOB_LEASE_TTL_SECS")
                .unwrap_or_else(|_| "120".to_string())
                .parse()
                .unwrap_or(120),
            long_term_retention_days: env::var("LONG_TERM_RETENTION_DAYS")
                .unwrap_or_else(|_| "365".to_string())
                .parse()
                .unwrap_or(365),
            autodream_enabled: env::var("AUTODREAM_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            autodream_interval_secs: env::var("AUTODREAM_INTERVAL_SECS")
                .unwrap_or_else(|_| "86400".to_string())
                .parse()
                .unwrap_or(86400),
            autodream_min_hours: env::var("AUTODREAM_MIN_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
            autodream_max_users_per_cycle: env::var("AUTODREAM_MAX_USERS_PER_CYCLE")
                .unwrap_or_else(|_| "8".to_string())
                .parse()
                .unwrap_or(8),
            autodream_channel_summaries: env::var("AUTODREAM_CHANNEL_SUMMARIES")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            autodream_max_channels_per_cycle: env::var("AUTODREAM_MAX_CHANNELS_PER_CYCLE")
                .unwrap_or_else(|_| "4".to_string())
                .parse()
                .unwrap_or(4),
            autodream_user_max_chars: env::var("AUTODREAM_USER_MAX_CHARS")
                .unwrap_or_else(|_| "1200".to_string())
                .parse()
                .unwrap_or(1200),
            autodream_activity_gate_hours: env::var("AUTODREAM_ACTIVITY_GATE_HOURS")
                .unwrap_or_else(|_| "48".to_string())
                .parse()
                .unwrap_or(48),
            autodream_activity_min_messages: env::var("AUTODREAM_ACTIVITY_MIN_MESSAGES")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            autodream_channel_activity_hours: env::var("AUTODREAM_CHANNEL_ACTIVITY_HOURS")
                .unwrap_or_else(|_| "72".to_string())
                .parse()
                .unwrap_or(72),
        })
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("discord_token", &"[REDACTED]")
            .field("application_id", &self.application_id)
            .field("owner_id", &self.owner_id)
            .field("llama_url", &self.llama_url)
            .field("llama_model", &self.llama_model)
            .field(
                "llama_api_key",
                &self.llama_api_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("embedding_url", &self.embedding_url)
            .field("embedding_model", &self.embedding_model)
            .field(
                "embedding_api_key",
                &self.embedding_api_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("database_url", &self.database_url)
            .field("system_prompt", &self.system_prompt)
            .field("max_context_messages", &self.max_context_messages)
            .field("status_message", &self.status_message)
            .field(
                "youtube_cookies",
                &self.youtube_cookies.as_ref().map(|_| "[REDACTED]"),
            )
            .field("searxng_url", &self.searxng_url)
            .field("web_tool_timeout_secs", &self.web_tool_timeout_secs)
            .field("web_search_default_limit", &self.web_search_default_limit)
            .field("web_fetch_max_chars", &self.web_fetch_max_chars)
            .field("jina_reader_base", &self.jina_reader_base)
            .field("context_message_limit", &self.context_message_limit)
            .field("context_retention_hours", &self.context_retention_hours)
            .field("llm_timeout_secs", &self.llm_timeout_secs)
            .field("embedding_timeout_secs", &self.embedding_timeout_secs)
            .field("voice_idle_timeout_secs", &self.voice_idle_timeout_secs)
            .field("voice_alone_timeout_secs", &self.voice_alone_timeout_secs)
            .field("voice_follow_user_move", &self.voice_follow_user_move)
            .field("max_queue_tracks", &self.max_queue_tracks)
            .field("voice_allow_duplicate_urls", &self.voice_allow_duplicate_urls)
            .field("dev_guild_id", &self.dev_guild_id)
            .field("register_commands", &self.register_commands)
            .field(
                "agent_confirm_timeout_secs",
                &self.agent_confirm_timeout_secs,
            )
            .field("embedding_indexer_enabled", &self.embedding_indexer_enabled)
            .field(
                "embedding_indexer_batch_size",
                &self.embedding_indexer_batch_size,
            )
            .field(
                "embedding_indexer_interval_secs",
                &self.embedding_indexer_interval_secs,
            )
            .field("summarization_enabled", &self.summarization_enabled)
            .field(
                "summarization_interval_secs",
                &self.summarization_interval_secs,
            )
            .field(
                "summarization_active_channels_lookback_days",
                &self.summarization_active_channels_lookback_days,
            )
            .field(
                "summarization_initial_min_messages",
                &self.summarization_initial_min_messages,
            )
            .field(
                "summarization_trigger_new_messages",
                &self.summarization_trigger_new_messages,
            )
            .field(
                "summarization_trigger_age_hours",
                &self.summarization_trigger_age_hours,
            )
            .field(
                "summarization_trigger_min_new_messages",
                &self.summarization_trigger_min_new_messages,
            )
            .field("summarization_max_tokens", &self.summarization_max_tokens)
            .field(
                "summarization_refresh_weeks",
                &self.summarization_refresh_weeks,
            )
            .field(
                "summarization_refresh_days_lookback",
                &self.summarization_refresh_days_lookback,
            )
            .field(
                "summarization_activity_gate_hours",
                &self.summarization_activity_gate_hours,
            )
            .field(
                "summarization_activity_min_messages",
                &self.summarization_activity_min_messages,
            )
            .field(
                "reminder_poll_interval_secs",
                &self.reminder_poll_interval_secs,
            )
            .field("reminder_batch_size", &self.reminder_batch_size)
            .field("health_port", &self.health_port)
            .field("job_leases_enabled", &self.job_leases_enabled)
            .field("job_lease_ttl_secs", &self.job_lease_ttl_secs)
            .field("long_term_retention_days", &self.long_term_retention_days)
            .field("autodream_enabled", &self.autodream_enabled)
            .field("autodream_interval_secs", &self.autodream_interval_secs)
            .field("autodream_min_hours", &self.autodream_min_hours)
            .field(
                "autodream_max_users_per_cycle",
                &self.autodream_max_users_per_cycle,
            )
            .field(
                "autodream_channel_summaries",
                &self.autodream_channel_summaries,
            )
            .field(
                "autodream_max_channels_per_cycle",
                &self.autodream_max_channels_per_cycle,
            )
            .field("autodream_user_max_chars", &self.autodream_user_max_chars)
            .field(
                "autodream_activity_gate_hours",
                &self.autodream_activity_gate_hours,
            )
            .field(
                "autodream_activity_min_messages",
                &self.autodream_activity_min_messages,
            )
            .field(
                "autodream_channel_activity_hours",
                &self.autodream_channel_activity_hours,
            )
            .finish()
    }
}

/// Discord message limit is 2000 characters
pub const DISCORD_MESSAGE_LIMIT: usize = 2000;
/// Embed description limit is 4096 characters  
pub const DISCORD_EMBED_LIMIT: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_config_logic() {
        // 1. Test missing vars
        env::remove_var("DISCORD_TOKEN");
        env::remove_var("APPLICATION_ID");
        let result = Config::build();
        assert!(
            result.is_err(),
            "Should fail when required vars are missing"
        );

        // 2. Test defaults
        env::set_var("DISCORD_TOKEN", "test_token");
        env::set_var("APPLICATION_ID", "12345");
        let config = Config::build().unwrap();
        assert_eq!(config.discord_token, "test_token");
        assert_eq!(config.application_id, 12345);

        // 3. Test debug redaction
        env::set_var("LLAMA_API_KEY", "secret_api_key");
        let config_redacted = Config::build().unwrap();
        let debug_output = format!("{:?}", config_redacted);
        assert!(!debug_output.contains("test_token"));
        assert!(!debug_output.contains("secret_api_key"));
        assert!(debug_output.contains("[REDACTED]"));

        // 4. Empty SYSTEM_PROMPT falls back to default (avoid blank bot)
        env::set_var("SYSTEM_PROMPT", "   ");
        let cfg = Config::build().unwrap();
        assert_eq!(cfg.system_prompt, DEFAULT_SYSTEM_PROMPT);

        // 5. MAX_QUEUE_TRACKS: 0 = unlimited; large values clamp to 500
        env::set_var("MAX_QUEUE_TRACKS", "99999");
        let c = Config::build().unwrap();
        assert_eq!(c.max_queue_tracks, 500);
        env::set_var("MAX_QUEUE_TRACKS", "0");
        let c = Config::build().unwrap();
        assert_eq!(c.max_queue_tracks, 0);
        env::remove_var("MAX_QUEUE_TRACKS");

        // Cleanup
        env::remove_var("DISCORD_TOKEN");
        env::remove_var("APPLICATION_ID");
        env::remove_var("LLAMA_API_KEY");
        env::remove_var("SYSTEM_PROMPT");
    }
}
