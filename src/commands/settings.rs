use crate::{Context, Error};
use poise::serenity_prelude as serenity;

/// Manage bot settings
#[poise::command(
    slash_command, 
    subcommands("context"),
    required_permissions = "MANAGE_GUILD",
    guild_only
)]
pub async fn settings(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Configure context persistence settings
#[poise::command(slash_command, subcommands("get", "set", "summarize"))]
pub async fn context(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Get current context settings
#[poise::command(slash_command)]
pub async fn get(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    
    // Get settings from DB
    let (db_limit, db_retention) = ctx.data().db.get_guild_settings(guild_id.get())?;
    
    // Fallback to config defaults
    let limit = db_limit.unwrap_or(ctx.data().config.context_message_limit);
    let retention = db_retention.unwrap_or(ctx.data().config.context_retention_hours);
    
    let source = if db_limit.is_none() && db_retention.is_none() {
        "Default Configuration"
    } else {
        "Server Custom Settings"
    };

    let embed = serenity::CreateEmbed::new()
        .title("🧠 Context Settings")
        .description(format!("Configuration for **{}**", ctx.guild().unwrap().name))
        .field("Message Limit", format!("`{}` messages", limit), true)
        .field("Retention", format!("`{}` hours", retention), true)
        .footer(serenity::CreateEmbedFooter::new(source))
        .color(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    
    Ok(())
}

/// Set context settings
#[poise::command(slash_command)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "Max messages to include in context"] 
    #[min = 1] #[max = 100]
    limit: Option<usize>,
    #[description = "Hours to retain messages"] 
    #[min = 1] #[max = 168]
    retention: Option<u64>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    
    if limit.is_none() && retention.is_none() {
        ctx.say("❌ Please specify at least one setting to change (limit or retention).").await?;
        return Ok(())
    }
    
    ctx.defer().await?;
    
    ctx.data().db.set_guild_settings(guild_id.get(), limit, retention)?;
    
    let mut confirmations = Vec::new();
    if let Some(l) = limit {
        confirmations.push(format!("limit to **{}** messages", l));
    }
    if let Some(r) = retention {
        confirmations.push(format!("retention to **{}** hours", r));
    }
    
    ctx.say(format!("✅ Updated context settings: set {}", confirmations.join(" and "))).await?;
    
    Ok(())
}

/// Manually trigger channel summarization
#[poise::command(slash_command)]
pub async fn summarize(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    
    let manager = crate::summarize::SummarizationManager::new(
        ctx.data().db.clone(),
        ctx.data().llm_client.clone()
    );
    
    manager.summarize_channel(&ctx.channel_id().to_string(), 7).await?;
    
    ctx.say("✅ Channel summarization complete.").await?;
    Ok(())
}
