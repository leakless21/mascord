pub mod cache;
pub mod commands;
pub mod config;
pub mod play_intent;
pub mod response_embed;
pub mod context;
pub mod db;
pub mod discord_text;
pub mod health;
pub mod indexer;
pub mod llm;
pub mod mention;
pub mod rag;
pub mod reminders;
pub mod reply;
pub mod services;
pub mod summarize;
pub mod system_prompt;
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
    /// Per-guild music (volume, queue loop, voice hook tracking).
    pub music: std::sync::Arc<crate::commands::music::MusicState>,
    /// Bot's own user ID for context formatting
    pub bot_id: u64,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
