use serde::Deserialize;
use dotenvy::dotenv;
use std::env;
use std::fs;
use crate::mcp::config::McpServerConfig;

#[derive(Debug, Clone, Deserialize)]
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
    pub mcp_servers: Vec<McpServerConfig>,
    // Context persistence settings
    pub context_message_limit: usize,
    pub context_retention_hours: u64,
}

const DEFAULT_SYSTEM_PROMPT: &str = "You are Mascord, a powerful and helpful Discord assistant. \
You have access to various tools and Model Context Protocol (MCP) servers to perform actions and fetch live data. \
When a user request requires action (like playing music, searching history, or fetching web content), you MUST use the appropriate tool. \
Be concise, accurate, and proactive in using your available capabilities.";

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
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            llama_model: env::var("LLAMA_MODEL")
                .unwrap_or_else(|_| "local-model".to_string()),
            llama_api_key: env::var("LLAMA_API_KEY").ok(),
            embedding_url: env::var("EMBEDDING_URL")
                .unwrap_or_else(|_| env::var("LLAMA_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())),
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
            mcp_servers: Self::load_mcp_servers()?,
            context_message_limit: env::var("CONTEXT_MESSAGE_LIMIT")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            context_retention_hours: env::var("CONTEXT_RETENTION_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
        })
    }

    pub fn load_mcp_servers() -> anyhow::Result<Vec<McpServerConfig>> {
        if let Ok(content) = fs::read_to_string("mcp_servers.toml") {
            if let Ok(servers) = toml::from_str(&content) {
                return Ok(servers);
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
        let content = toml::to_string(servers)?;
        fs::write("mcp_servers.toml", content)?;
        Ok(())
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
    fn test_config_defaults() {
        env::set_var("DISCORD_TOKEN", "test_token");
        env::set_var("APPLICATION_ID", "12345");
        
        let config = Config::build().unwrap();
        
        assert_eq!(config.discord_token, "test_token");
        assert_eq!(config.application_id, 12345);
        
        env::remove_var("DISCORD_TOKEN");
        env::remove_var("APPLICATION_ID");
    }

    #[test]
    fn test_config_missing_vars() {
        env::remove_var("DISCORD_TOKEN");
        env::remove_var("APPLICATION_ID");
        let result = Config::build();
        assert!(result.is_err());
    }
}
