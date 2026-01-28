use crate::tools::Tool;
use async_trait::async_trait;
use serde_json::{Value, json};

pub struct ShutdownTool;

#[async_trait]
impl Tool for ShutdownTool {
    fn name(&self) -> &str { "shutdown_bot" }
    fn description(&self) -> &str { "Gracefully shut down the bot (Owner only)" }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }
    async fn execute(&self, _params: Value) -> anyhow::Result<Value> {
        // Implementation will trigger shutdown
        Ok(json!({"status": "error", "message": "Not yet implemented"}))
    }
}
