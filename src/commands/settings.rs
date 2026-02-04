use crate::{Context, Error};
use poise::serenity_prelude as serenity;
use tracing::info;

/// Manage bot settings
#[poise::command(
    slash_command,
    subcommands("context", "memory", "system_prompt", "agent_timeout", "voice_timeout"),
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

/// Manage per-channel memory settings (list, enable, disable, scope, purge)
#[poise::command(
    slash_command,
    subcommands("list", "enable", "disable", "scope", "purge")
)]
pub async fn memory(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// View or update the server system prompt
#[poise::command(slash_command)]
pub async fn system_prompt(
    ctx: Context<'_>,
    #[description = "New system prompt (omit to view current)"] prompt: Option<String>,
    #[description = "Reset to default config value"] reset: Option<bool>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;

    if reset.unwrap_or(false) {
        ctx.data()
            .db
            .set_guild_system_prompt(guild_id.get(), None)?;
        ctx.say("✅ System prompt reset to default.").await?;
        return Ok(());
    }

    if let Some(prompt) = prompt {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            ctx.say("❌ System prompt cannot be empty.").await?;
            return Ok(());
        }
        ctx.data()
            .db
            .set_guild_system_prompt(guild_id.get(), Some(trimmed))?;
        ctx.say("✅ System prompt updated for this server.").await?;
        return Ok(());
    }

    let override_prompt = ctx.data().db.get_guild_system_prompt(guild_id.get())?;
    let (prompt_text, source) = match override_prompt {
        Some(p) if !p.trim().is_empty() => (p, "Server Override"),
        _ => (ctx.data().config.system_prompt.clone(), "Default Configuration"),
    };

    let embed = serenity::CreateEmbed::new()
        .title("🧠 System Prompt")
        .description(prompt_text)
        .footer(serenity::CreateEmbedFooter::new(source))
        .color(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// View or update the agent confirmation timeout
#[poise::command(slash_command)]
pub async fn agent_timeout(
    ctx: Context<'_>,
    #[description = "Tool confirmation timeout in seconds (omit to view)"]
    #[min = 30]
    #[max = 3600]
    timeout_secs: Option<u64>,
    #[description = "Reset to default config value"] reset: Option<bool>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;

    if reset.unwrap_or(false) {
        ctx.data()
            .db
            .set_guild_agent_confirm_timeout(guild_id.get(), None)?;
        ctx.say("✅ Agent confirmation timeout reset to default.")
            .await?;
        return Ok(());
    }

    if let Some(timeout) = timeout_secs {
        ctx.data()
            .db
            .set_guild_agent_confirm_timeout(guild_id.get(), Some(timeout))?;
        ctx.say(format!(
            "✅ Agent confirmation timeout set to **{}** seconds.",
            timeout
        ))
        .await?;
        return Ok(());
    }

    let override_timeout = ctx
        .data()
        .db
        .get_guild_agent_confirm_timeout(guild_id.get())?;
    let timeout = override_timeout.unwrap_or(ctx.data().config.agent_confirm_timeout_secs);
    let source = if override_timeout.is_some() {
        "Server Override"
    } else {
        "Default Configuration"
    };

    let embed = serenity::CreateEmbed::new()
        .title("⏱️ Agent Confirmation Timeout")
        .description(format!("**{}** seconds", timeout))
        .footer(serenity::CreateEmbedFooter::new(source))
        .color(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// View or update voice idle timeout
#[poise::command(slash_command)]
pub async fn voice_timeout(
    ctx: Context<'_>,
    #[description = "Voice idle timeout in seconds (omit to view)"]
    #[min = 60]
    #[max = 3600]
    timeout_secs: Option<u64>,
    #[description = "Reset to default config value"] reset: Option<bool>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;

    if reset.unwrap_or(false) {
        ctx.data()
            .db
            .set_guild_voice_idle_timeout(guild_id.get(), None)?;
        ctx.say("✅ Voice idle timeout reset to default.").await?;
        return Ok(());
    }

    if let Some(timeout) = timeout_secs {
        ctx.data()
            .db
            .set_guild_voice_idle_timeout(guild_id.get(), Some(timeout))?;
        ctx.say(format!(
            "✅ Voice idle timeout set to **{}** seconds.",
            timeout
        ))
        .await?;
        return Ok(());
    }

    let override_timeout = ctx
        .data()
        .db
        .get_guild_voice_idle_timeout(guild_id.get())?;
    let timeout = override_timeout.unwrap_or(ctx.data().config.voice_idle_timeout_secs);
    let source = if override_timeout.is_some() {
        "Server Override"
    } else {
        "Default Configuration"
    };

    let embed = serenity::CreateEmbed::new()
        .title("🔊 Voice Idle Timeout")
        .description(format!("**{}** seconds", timeout))
        .footer(serenity::CreateEmbedFooter::new(source))
        .color(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// List all per-channel memory settings
#[poise::command(slash_command)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    let settings = ctx.data().db.list_channel_settings(&guild_id.to_string())?;

    if settings.is_empty() {
        ctx.say("📭 No custom channel settings found. All channels are using default (enabled).")
            .await?;
        return Ok(());
    }

    let mut description = String::from("Custom memory settings for this server:\n\n");
    for (channel_id, enabled, scope_date) in settings {
        let status = if enabled {
            "✅ Enabled"
        } else {
            "❌ Disabled"
        };
        let scope_str = scope_date.unwrap_or_else(|| "Full History".to_string());
        description.push_str(&format!("<#{}> | {} | {}\n", channel_id, status, scope_str));
    }

    let embed = serenity::CreateEmbed::new()
        .title("🧠 Channel Memory Settings")
        .description(description)
        .color(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Enable tracking for a channel
#[poise::command(slash_command)]
pub async fn enable(
    ctx: Context<'_>,
    #[description = "The channel to enable tracking for"] target_channel: serenity::Channel,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    ctx.data().db.set_channel_enabled(
        &guild_id.to_string(),
        &target_channel.id().to_string(),
        true,
    )?;
    ctx.say(format!(
        "✅ Memory tracking enabled for <#{}>.",
        target_channel.id()
    ))
    .await?;
    Ok(())
}

/// Disable tracking for a channel
#[poise::command(slash_command)]
pub async fn disable(
    ctx: Context<'_>,
    #[description = "The channel to disable tracking for"] target_channel: serenity::Channel,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    ctx.data().db.set_channel_enabled(
        &guild_id.to_string(),
        &target_channel.id().to_string(),
        false,
    )?;
    ctx.say(format!(
        "❌ Memory tracking disabled for <#{}>. Messages in this channel will no longer be saved.",
        target_channel.id()
    ))
    .await?;
    Ok(())
}

/// Set memory scope (start date) for a channel
#[poise::command(slash_command)]
pub async fn scope(
    ctx: Context<'_>,
    #[description = "The channel to configure scope for"] target_channel: serenity::Channel,
    #[description = "Start date (YYYY-MM-DD HH:MM:SS) or 'none' to reset"] date: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    let date_val = if date.to_lowercase() == "none" {
        None
    } else {
        Some(date)
    };

    ctx.data().db.set_channel_memory_scope(
        &guild_id.to_string(),
        &target_channel.id().to_string(),
        date_val.clone(),
    )?;

    match date_val {
        Some(d) => {
            ctx.say(format!(
                "✅ Memory scope for <#{}> set to start from **{}**.",
                target_channel.id(),
                d
            ))
            .await?
        }
        None => {
            ctx.say(format!(
                "✅ Memory scope for <#{}> reset to full history.",
                target_channel.id()
            ))
            .await?
        }
    };
    Ok(())
}

/// Purge messages for a channel
#[poise::command(slash_command)]
pub async fn purge(
    ctx: Context<'_>,
    #[description = "The channel to purge messages from"] channel: serenity::Channel,
    #[description = "Optional: Purge messages BEFORE this date (YYYY-MM-DD HH:MM:SS)"]
    before_date: Option<String>,
) -> Result<(), Error> {
    let _guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;

    // Request confirmation
    let confirm_msg = match &before_date {
        Some(d) => format!("⚠️ Are you sure you want to purge messages in <#{}> before **{}**? This cannot be undone.", channel.id(), d),
        None => format!("⚠️ Are you sure you want to purge **ALL** messages in <#{}>? This cannot be undone.", channel.id()),
    };

    let _ctx_id = ctx.id();
    let reply = ctx
        .send(
            poise::CreateReply::default()
                .content(confirm_msg)
                .components(vec![serenity::CreateActionRow::Buttons(vec![
                    serenity::CreateButton::new("confirm")
                        .label("Confirm Purge")
                        .style(serenity::ButtonStyle::Danger),
                    serenity::CreateButton::new("cancel")
                        .label("Cancel")
                        .style(serenity::ButtonStyle::Secondary),
                ])]),
        )
        .await?;

    if let Some(interaction) = reply
        .message()
        .await?
        .await_component_interaction(ctx.serenity_context())
        .author_id(ctx.author().id)
        .await
    {
        if interaction.data.custom_id == "confirm" {
            let count = ctx
                .data()
                .db
                .purge_messages(&channel.id().to_string(), before_date)?;
            interaction
                .create_response(
                    ctx.serenity_context(),
                    serenity::CreateInteractionResponse::UpdateMessage(
                        serenity::CreateInteractionResponseMessage::new()
                            .content(format!(
                                "🗑️ Purged **{}** messages from <#{}>.",
                                count,
                                channel.id()
                            ))
                            .components(vec![]),
                    ),
                )
                .await?;
        } else {
            interaction
                .create_response(
                    ctx.serenity_context(),
                    serenity::CreateInteractionResponse::UpdateMessage(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("❌ Purge cancelled.")
                            .components(vec![]),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}

/// Get current context settings
#[poise::command(slash_command)]
pub async fn get(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    info!(
        "Settings get command: Fetching context settings for guild {}",
        guild_id
    );

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

    let guild_name = ctx
        .guild()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| "this server".to_string());

    let embed = serenity::CreateEmbed::new()
        .title("🧠 Context Settings")
        .description(format!("Configuration for **{}**", guild_name))
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
    #[min = 1]
    #[max = 100]
    limit: Option<usize>,
    #[description = "Hours to retain messages"]
    #[min = 1]
    #[max = 168]
    retention: Option<u64>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;

    if limit.is_none() && retention.is_none() {
        ctx.say("❌ Please specify at least one setting to change (limit or retention).")
            .await?;
        return Ok(());
    }

    ctx.defer().await?;

    ctx.data()
        .db
        .set_guild_settings(guild_id.get(), limit, retention)?;

    let mut confirmations = Vec::new();
    if let Some(l) = limit {
        confirmations.push(format!("limit to **{}** messages", l));
    }
    if let Some(r) = retention {
        confirmations.push(format!("retention to **{}** hours", r));
    }

    info!(
        "Settings set command for guild {}: updated {}",
        guild_id,
        confirmations.join(", ")
    );
    ctx.say(format!(
        "✅ Updated context settings: set {}",
        confirmations.join(" and ")
    ))
    .await?;

    Ok(())
}

/// Manually trigger channel summarization
#[poise::command(slash_command)]
pub async fn summarize(ctx: Context<'_>) -> Result<(), Error> {
    info!(
        "Manual summarization triggered for channel {} by user {}",
        ctx.channel_id(),
        ctx.author().name
    );
    ctx.defer().await?;

    let manager = crate::summarize::SummarizationManager::new(
        ctx.data().db.clone(),
        ctx.data().llm_client.clone(),
        &ctx.data().config,
    );

    manager
        .summarize_channel(&ctx.channel_id().to_string(), 7)
        .await?;

    info!(
        "Manual summarization for channel {} completed successfully",
        ctx.channel_id()
    );
    ctx.say("✅ Channel summarization complete.").await?;
    Ok(())
}
