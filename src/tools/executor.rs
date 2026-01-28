use std::sync::Arc;
use crate::tools::{ToolRegistry, Tool};
use serde_json::Value;

pub struct ToolExecutor {
    registry: Arc<ToolRegistry>,
}

impl ToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    pub async fn execute(&self, name: &str, params: Value) -> anyhow::Result<Value> {
        let tool = self.registry.get(name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;
            
        tool.execute(params).await
    }
}
