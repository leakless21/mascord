use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub mod builtin;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    fn requires_confirmation(&self) -> bool {
        false
    }
    async fn execute(&self, params: Value) -> anyhow::Result<Value>;

    /// When the agent runs with [`crate::llm::confirm::ToolConfirmationContext`], this is called with
    /// guild/channel/user and [`crate::Data`] (e.g. [`crate::tools::builtin::music::MusicTool`]).
    async fn execute_with_discord(
        &self,
        params: Value,
        dctx: Option<&crate::llm::confirm::DiscordToolContext<'_>>,
    ) -> anyhow::Result<Value> {
        let _ = dctx;
        self.execute(params).await
    }
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    pub fn get_definitions(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameters_schema()
                    }
                })
            })
            .collect()
    }
}
