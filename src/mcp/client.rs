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
use tracing::{info, debug, warn, error};

pub struct McpClientManager {
    services: Arc<Mutex<HashMap<String, Arc<RunningService<RoleClient, ()>>>>>,
    timeout_secs: u64,
}

impl McpClientManager {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            services: Arc::new(Mutex::new(HashMap::new())),
            timeout_secs: config.mcp_timeout_secs,
        })
    }

    pub async fn connect(&self, config: &McpServerConfig) -> Result<()> {
        let mut services_lock = self.services.lock().await;
        if services_lock.contains_key(&config.name) {
            debug!("MCP server '{}' already connected", config.name);
            return Ok(());
        }

        info!("MCP client: Connecting to server '{}' via {:?}...", config.name, config.transport);
        let running = match config.transport {
            McpTransport::Stdio => {
                let mut cmd = Command::new(config.command.as_ref().ok_or_else(|| {
                    error!("MCP client: Command not specified for stdio transport on server '{}'", config.name);
                    anyhow!("Command not specified for stdio transport")
                })?);
                if let Some(args) = &config.args {
                    cmd.args(args);
                }
                if let Some(env) = &config.env {
                    cmd.envs(env);
                }
                
                let transport = TokioChildProcess::new(&mut cmd).map_err(|e| {
                    error!("MCP client: Failed to start child process for server '{}': {}", config.name, e);
                    e
                })?;
                ().serve(transport).await.map_err(|e| {
                    error!("MCP client: Failed to serve transport for server '{}': {}", config.name, e);
                    e
                })?
            }
            McpTransport::Sse => {
                warn!("MCP client: SSE transport requested for server '{}' but not implemented", config.name);
                return Err(anyhow!("SSE transport not yet implemented"));
            }
        };

        info!("MCP client: Successfully connected to server '{}'", config.name);
        services_lock.insert(config.name.clone(), Arc::new(running));
        Ok(())
    }

    pub async fn disconnect(&self, name: &str) -> Result<()> {
        let mut services_lock = self.services.lock().await;
        if services_lock.remove(name).is_some() {
            info!("MCP client: Disconnected from server '{}'", name);
            Ok(())
        } else {
            error!("MCP client: Failed to disconnect - server '{}' not found", name);
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
            debug!("MCP client: Discovering tools from server '{}'...", server_name);
            if let Ok(tools_result) = service.list_tools(Default::default()).await {
                debug!("MCP client: Found {} tools on server '{}'", tools_result.tools.len(), server_name);
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
                        timeout_secs: self.timeout_secs,
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
    timeout_secs: u64,
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
        use tokio::time::{timeout, Duration};
        
        debug!("MCP tool client: Executing '{}' on server '{}'...", self.name, self.server_name);
        
        let result = timeout(Duration::from_secs(self.timeout_secs), self.service.call_tool(CallToolRequestParam {
            name: self.name.clone().into(),
            arguments: params.as_object().cloned(),
        }))
        .await
        .map_err(|_| {
            error!("MCP tool '{}' timed out after {}s", self.name, self.timeout_secs);
            anyhow!("MCP tool '{}' timed out after {}s", self.name, self.timeout_secs)
        })??;
        
        info!("MCP tool '{}' on server '{}' executed successfully", self.name, self.server_name);
        Ok(serde_json::to_value(result)?)
    }
}
