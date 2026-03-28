pub mod agent;
pub mod client;
pub mod confirm;

pub use client::LlmClient;

/// User-visible assistant error (anyhow [`Display`](std::fmt::Display) chain).
pub fn format_assistant_error(e: &anyhow::Error) -> String {
    e.to_string()
}
