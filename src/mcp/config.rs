use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Sse,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

impl<'de> Deserialize<'de> for McpServerConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawMcpServerConfig {
            name: String,
            #[serde(default)]
            transport: Option<String>,
            command: Option<String>,
            args: Option<Vec<String>>,
            url: Option<String>,
            env: Option<HashMap<String, String>>,
        }

        let raw = RawMcpServerConfig::deserialize(deserializer)?;

        let transport_raw = raw.transport.unwrap_or_else(|| "stdio".to_string());
        let transport = match transport_raw.to_lowercase().as_str() {
            "stdio" | "child_process" | "child-process" => McpTransport::Stdio,
            "sse" => McpTransport::Sse,
            // Back-compat: many MCP examples call this "http" but use stdio-based servers.
            "http" | "https" => {
                if raw.url.is_some() {
                    McpTransport::Sse
                } else {
                    McpTransport::Stdio
                }
            }
            _ => McpTransport::Stdio,
        };

        Ok(Self {
            name: raw.name,
            transport,
            command: raw.command,
            args: raw.args,
            url: raw.url,
            env: raw.env,
        })
    }
}
