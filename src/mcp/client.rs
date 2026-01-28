use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::{Result, anyhow};
use serde_json::Value;
use tokio::process::Command;
use rmcp::{
    model::CallToolRequestParam,
    service::{ServiceExt, RoleClient, RunningService},
    transport::child_process::TokioChildProcess,
};
use crate::config::Config;
use crate::mcp::config::{McpServerConfig, McpTransport};
use crate::tools::Tool;
use async_trait::async_trait;

pub struct McpClientManager {
    services: Arc<Mutex<HashMap<String, Arc<RunningService<RoleClient, ()>>>>>,
}

impl McpClientManager {
    pub fn new(_config: &Config) -> Result<Self> {
        Ok(Self {
            services: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn connect(&self, config: &McpServerConfig) -> Result<()> {
        let mut services_lock = self.services.lock().await;
        if services_lock.contains_key(&config.name) {
            return Ok(());
        }

        let running = match config.transport {
            McpTransport::Stdio => {
                let mut cmd = Command::new(config.command.as_ref().ok_or_else(|| anyhow!("Command not specified for stdio transport"))?);
                if let Some(args) = &config.args {
                    cmd.args(args);
                }
                if let Some(env) = &config.env {
                    cmd.envs(env);
                }
                
                let transport = TokioChildProcess::new(&mut cmd)?;
                ().serve(transport).await?
            }
            McpTransport::Sse => {
                return Err(anyhow!("SSE transport not yet implemented"));
            }
        };

        services_lock.insert(config.name.clone(), Arc::new(running));
        Ok(())
    }

    pub async fn disconnect(&self, name: &str) -> Result<()> {
        let mut services_lock = self.services.lock().await;
        if services_lock.remove(name).is_some() {
            Ok(())
        } else {
            Err(anyhow!("Server not found: {}", name))
        }
    }

    pub async fn list_active_servers(&self) -> Vec<String> {
        let services_lock = self.services.lock().await;
        services_lock.keys().cloned().collect()
    }

    pub async fn list_all_tools(&self) -> Vec<Arc<dyn Tool>> {
        let services = self.services.lock().await;
        let mut all_tools = Vec::new();

        for (server_name, service) in services.iter() {
            if let Ok(tools_result) = service.list_tools(Default::default()).await {
                for tool in tools_result.tools {
                    // Assuming description might be Cow or Option<Cow> based on build errors
                    // In current version it seems to be Cow.
                    let desc = tool.description.to_string();
                    
                    all_tools.push(Arc::new(McpToolWrapper {
                        server_name: server_name.clone(),
                        service: service.clone(),
                        name: tool.name.to_string(),
                        description: desc,
                        input_schema: serde_json::Value::Object((*tool.input_schema).clone()),
                    }) as Arc<dyn Tool>);
                }
            }
        }
        all_tools
    }
}

pub struct McpToolWrapper {
    server_name: String,
    service: Arc<RunningService<RoleClient, ()>>,
    name: String,
    description: String,
    input_schema: Value,
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.input_schema.clone()
    }

    async fn execute(&self, params: Value) -> Result<Value> {
        let result = self.service.call_tool(CallToolRequestParam {
            name: self.name.clone().into(),
            arguments: params.as_object().cloned(),
        }).await?;
        
        Ok(serde_json::to_value(result)?)
    }
}
