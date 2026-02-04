use crate::mcp::config::McpServerConfig;
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;

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
    pub mcp_servers: Vec<McpServerConfig>,
    // Context persistence settings
    pub context_message_limit: usize,
    pub context_retention_hours: u64,
    // Timeout & Maintenance settings
    pub llm_timeout_secs: u64,
    pub embedding_timeout_secs: u64,
    pub mcp_timeout_secs: u64,
    pub voice_idle_timeout_secs: u64,
    pub dev_guild_id: Option<u64>,
    pub register_commands: bool,
    pub mcp_tools_require_confirmation: bool,

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

    // Long-term retention (RAG store)
    pub long_term_retention_days: u64,
}

const DEFAULT_SYSTEM_PROMPT: &str = "You are Mascord, a powerful and helpful Discord assistant. \
You have access to various tools and Model Context Protocol (MCP) servers to perform actions and fetch live data. \
When a user request requires action (like playing music, searching history, or fetching web content), you MUST use the appropriate tool. \
Be concise, accurate, and proactive in using your available capabilities. Be a little snarky!";

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
                .unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string()),
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
            mcp_servers: Self::load_mcp_servers()?,
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
            mcp_timeout_secs: env::var("MCP_TIMEOUT_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            voice_idle_timeout_secs: env::var("VOICE_IDLE_TIMEOUT_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),
            dev_guild_id: env::var("DEV_GUILD_ID").ok().and_then(|id| id.parse().ok()),
            register_commands: env::var("REGISTER_COMMANDS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            mcp_tools_require_confirmation: env::var("MCP_TOOLS_REQUIRE_CONFIRMATION")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

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
            long_term_retention_days: env::var("LONG_TERM_RETENTION_DAYS")
                .unwrap_or_else(|_| "365".to_string())
                .parse()
                .unwrap_or(365),
        })
    }

    pub fn load_mcp_servers() -> anyhow::Result<Vec<McpServerConfig>> {
        if let Ok(content) = fs::read_to_string("mcp_servers.toml") {
            #[derive(Deserialize)]
            struct McpWrapper {
                servers: Vec<McpServerConfig>,
            }
            if let Ok(wrapper) = toml::from_str::<McpWrapper>(&content) {
                return Ok(wrapper.servers);
            }
        }

        // Fallback to env variable
        if let Ok(env_servers) = env::var("MCP_SERVERS") {
            if let Ok(servers) = serde_json::from_str(&env_servers) {
                return Ok(servers);
            }
        }

        Ok(Vec::new())
    }

    pub fn save_mcp_servers(servers: &[McpServerConfig]) -> anyhow::Result<()> {
        #[derive(Serialize)]
        struct McpWrapper<'a> {
            servers: &'a [McpServerConfig],
        }
        let wrapper = McpWrapper { servers };
        let content = toml::to_string(&wrapper)?;
        fs::write("mcp_servers.toml", content)?;
        Ok(())
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
            .field("mcp_servers", &self.mcp_servers)
            .field("context_message_limit", &self.context_message_limit)
            .field("context_retention_hours", &self.context_retention_hours)
            .field("llm_timeout_secs", &self.llm_timeout_secs)
            .field("embedding_timeout_secs", &self.embedding_timeout_secs)
            .field("mcp_timeout_secs", &self.mcp_timeout_secs)
            .field("voice_idle_timeout_secs", &self.voice_idle_timeout_secs)
            .field("dev_guild_id", &self.dev_guild_id)
            .field("register_commands", &self.register_commands)
            .field(
                "mcp_tools_require_confirmation",
                &self.mcp_tools_require_confirmation,
            )
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
            .field("long_term_retention_days", &self.long_term_retention_days)
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

        // Cleanup
        env::remove_var("DISCORD_TOKEN");
        env::remove_var("APPLICATION_ID");
        env::remove_var("LLAMA_API_KEY");
    }
}
