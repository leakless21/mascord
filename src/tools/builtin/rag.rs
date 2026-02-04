use crate::db::Database;
use crate::tools::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct SearchLocalHistoryTool {
    pub db: Database,
    pub llm: crate::llm::LlmClient,
}

#[async_trait]
impl Tool for SearchLocalHistoryTool {
    fn name(&self) -> &str {
        "search_local_history"
    }
    fn description(&self) -> &str {
        "Search past Discord messages in this server. Use this when the user asks about past events or conversations that are not in the current context."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query containing keywords"
                }
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, params: Value) -> anyhow::Result<Value> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

        let filter = crate::rag::SearchFilter::default().with_limit(5);

        // Prefer semantic search when embeddings are available; fall back to keyword search if embedding fails.
        let embedding = self.llm.get_embeddings(query).await.unwrap_or_default();
        let results = self.db.search_messages(query, embedding, filter).await?;

        if results.is_empty() {
            return Ok(json!({"result": "No messages found matching the query."}));
        }

        let mut raw_history = String::new();
        for msg in &results {
            raw_history.push_str(&format!(
                "[{}] (channel {}) {}: {}\n",
                msg.timestamp, msg.channel_id, msg.user_id, msg.content
            ));
        }

        // Summarize the findings for the agent
        let prompt = format!(
            "Summarize the following search results from Discord history based on the user's query: '{}'. \
             Focus on the most relevant information.\n\nResults:\n{}",
            query, raw_history
        );

        let result_summary = self.llm.completion(&prompt).await?;

        Ok(json!({
            "result": result_summary,
            "raw_count": results.len(),
            "sources": results.iter().map(|msg| json!({
                "timestamp": msg.timestamp,
                "channel_id": msg.channel_id,
                "user_id": msg.user_id,
                "content": msg.content
            })).collect::<Vec<_>>()
        }))
    }
}
