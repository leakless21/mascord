use crate::db::Database;
use crate::llm::LlmClient;
use crate::rag::SearchFilter;
use chrono::{Utc, Duration};
use tracing::info;

pub struct SummarizationManager {
    db: Database,
    llm: LlmClient,
}

impl SummarizationManager {
    pub fn new(db: Database, llm: LlmClient) -> Self {
        Self { db, llm }
    }

    /// Summarizes the last N messages for a channel and saves the summary
    pub async fn summarize_channel(&self, channel_id: &str, days: i64) -> anyhow::Result<()> {
        info!("Starting summarization for channel: {}", channel_id);

        // 1. Fetch messages from DB for the last N days
        let filter = SearchFilter::default()
            .with_channel(channel_id.to_string())
            .with_from_date(Utc::now() - Duration::days(days))
            .with_limit(200); // Summary based on up to 200 messages

        // We use the db search method, but we need raw messages for summarization
        // Let's use a simpler query directly or assume search_messages is enough.
        // Actually, search_messages is for embeddings. 
        // Let's add a get_recent_messages to Database if needed.
        // For now, let's use search_messages with an empty embedding (keyword fallback will return all if query is empty)
        let messages = self.db.search_messages("", vec![], filter).await?;

        if messages.is_empty() {
            info!("No messages to summarize for channel: {}", channel_id);
            return Ok(());
        }

        // 2. Format messages for the summarizer
        let mut text_to_summarize = String::new();
        for msg in messages.iter().rev() { // chronological
            text_to_summarize.push_str(&format!("[{}] {}: {}\n", msg.timestamp, msg.user_id, msg.content));
        }

        // 3. Prompt the LLM
        let prompt = format!(
            "Please provide a concise but comprehensive summary of the following chat history. \
             Focus on key decisions, projects discussed, and significant events. \
             Keep the summary under 300 words.\n\nHistory:\n{}",
            text_to_summarize
        );

        let summary = self.llm.completion(&prompt).await?;

        // 4. Save to DB
        self.db.save_summary(channel_id, &summary)?;

        info!("Successfully summarized channel: {}", channel_id);
        Ok(())
    }
}
