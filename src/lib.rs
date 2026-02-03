pub mod cache;
pub mod commands;
pub mod config;
pub mod context;
pub mod db;
pub mod discord_text;
pub mod indexer;
pub mod llm;
pub mod mcp;
pub mod mention;
pub mod rag;
pub mod reply;
pub mod summarize;
pub mod tools;
pub mod voice;

/// Custom data passed to all commands
pub struct Data {
    pub config: config::Config,
    pub http_client: reqwest::Client,
    pub llm_client: llm::LlmClient,
    pub db: db::Database,
    pub cache: cache::MessageCache,
    pub tools: std::sync::Arc<tools::ToolRegistry>,
    pub mcp_manager: std::sync::Arc<mcp::client::McpClientManager>,
    /// Bot's own user ID for context formatting
    pub bot_id: u64,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
