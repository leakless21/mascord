pub mod config;
pub mod commands;
pub mod llm;
pub mod rag;
pub mod voice;
pub mod db;
pub mod cache;

/// Custom data passed to all commands
pub struct Data {
    pub config: config::Config,
    pub http_client: reqwest::Client,
    pub llm_client: llm::LlmClient,
    pub db: db::Database,
    pub cache: cache::MessageCache,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
