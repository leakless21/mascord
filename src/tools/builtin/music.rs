use crate::tools::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct PlayMusicTool;

#[async_trait]
impl Tool for PlayMusicTool {
    fn name(&self) -> &str {
        "play_music"
    }
    fn description(&self) -> &str {
        "Play music from a YouTube URL or search query"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The song title or YouTube URL to play"
                }
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, _params: Value) -> anyhow::Result<Value> {
        // Implementation will call existing music service
        Ok(json!({"status": "error", "message": "Not yet implemented"}))
    }
}
