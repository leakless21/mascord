use crate::{Context, Error};
use crate::mcp::config::{McpServerConfig, McpTransport};
use crate::config::Config;
use poise::serenity_prelude as serenity;
use tracing::{info, warn};

/// Manage MCP servers
#[poise::command(slash_command, subcommands("list", "add", "remove"), check = "is_owner")]
pub async fn mcp(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

async fn is_owner(ctx: Context<'_>) -> Result<bool, Error> {
    let owner_id = ctx.data().config.owner_id;
    if let Some(owner_id) = owner_id {
        if ctx.author().id == serenity::UserId::new(owner_id) {
            return Ok(true);
        }
    }
    
    ctx.say("❌ Only the bot owner can manage MCP servers.").await?;
    Ok(false)
}

/// List all configured MCP servers
#[poise::command(slash_command)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    info!("MCP list command received from {}", ctx.author().name);
    let active_servers = ctx.data().mcp_manager.list_active_servers().await;
    let all_configured = &ctx.data().config.mcp_servers;
    
    let mut response = String::from("## Configured MCP Servers\n");
    if all_configured.is_empty() {
        response.push_str("_No servers configured._");
    } else {
        for server in all_configured {
            let status = if active_servers.contains(&server.name) {
                "🟢 Active"
            } else {
                "🔴 Offline"
            };
            response.push_str(&format!("- **{}**: {} ({:?})\n", server.name, status, server.transport));
        }
    }
    
    ctx.say(response).await?;
    Ok(())
}

/// Add a new stdio-based MCP server
#[poise::command(slash_command)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Unique name for the server"] name: String,
    #[description = "Command to run (e.g., npx)"] command: String,
    #[description = "Arguments (comma separated)"] args: Option<String>,
) -> Result<(), Error> {
    let args_vec = args.map(|s| s.split(',').map(|a| a.trim().to_string()).collect());
    
    let new_server = McpServerConfig {
        name: name.clone(),
        transport: McpTransport::Stdio,
        command: Some(command),
        args: args_vec,
        url: None,
        env: None,
    };
    
    // 1. Connect to the new server
    ctx.data().mcp_manager.connect(&new_server).await?;
    
    // 2. Update and persist configuration (This part is tricky because Data.config is not mutable)
    // We should probably have a way to update the global config or just rely on the TOML file.
    // For now, let's update the TOML file. Next restart it will be fully integrated.
    // However, to make it work NOW, we already connected it to the manager.
    
    let mut current_servers = Config::load_mcp_servers().unwrap_or_default();
    current_servers.retain(|s| s.name != name);
    current_servers.push(new_server);
    Config::save_mcp_servers(&current_servers)?;
    
    info!("MCP add command: Successfully added and connected to server '{}'", name);
    ctx.say(format!("✅ Successfully added and connected to MCP server: **{}**", name)).await?;
    Ok(())
}

/// Remove an MCP server
#[poise::command(slash_command)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Name of the server to remove"] name: String,
) -> Result<(), Error> {
    info!("MCP remove command: Attempting to remove server '{}'", name);
    // 1. Disconnect from manager
    let _ = ctx.data().mcp_manager.disconnect(&name).await;
    
    // 2. Update persistence
    let mut current_servers = Config::load_mcp_servers().unwrap_or_default();
    let initial_len = current_servers.len();
    current_servers.retain(|s| s.name != name);
    
    if current_servers.len() < initial_len {
        Config::save_mcp_servers(&current_servers)?;
        info!("MCP remove command: Successfully removed server '{}' from configuration", name);
        ctx.say(format!("✅ Successfully removed MCP server: **{}**", name)).await?;
    } else {
        warn!("MCP remove command: Server '{}' not found in configuration", name);
        ctx.say(format!("❌ MCP server **{}** not found in configuration.", name)).await?;
    }
    
    Ok(())
}
