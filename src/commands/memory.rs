use crate::services::user_memory::UserMemoryService;
use crate::{Context, Error};
use poise::serenity_prelude as serenity;
use tracing::info;

/// Manage your global opt-in memory profile
#[poise::command(
    slash_command,
    subcommands("enable", "disable", "show", "remember", "forget", "delete_data")
)]
pub async fn memory(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Enable your global memory profile
#[poise::command(slash_command)]
pub async fn enable(ctx: Context<'_>) -> Result<(), Error> {
    let service = UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
    service
        .set_user_memory_enabled(ctx.author().id.get(), true)
        .await?;
    ctx.say("✅ Your global memory profile is enabled.").await?;
    Ok(())
}

/// Disable your global memory profile
#[poise::command(slash_command)]
pub async fn disable(ctx: Context<'_>) -> Result<(), Error> {
    let service = UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
    service
        .set_user_memory_enabled(ctx.author().id.get(), false)
        .await?;
    ctx.say("✅ Your global memory profile is disabled.")
        .await?;
    Ok(())
}

/// View your global memory profile
#[poise::command(slash_command)]
pub async fn show(ctx: Context<'_>) -> Result<(), Error> {
    let service = UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
    let record = service
        .get_user_memory_record(ctx.author().id.get())
        .await?;

    let Some(record) = record else {
        ctx.say("📭 No memory profile found. Use `/memory remember` to create one.")
            .await?;
        return Ok(());
    };

    let status = if record.enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    let expires = record
        .expires_at
        .clone()
        .unwrap_or_else(|| "Never".to_string());
    let description = if record.summary.trim().is_empty() {
        "No memory summary saved yet. Use `/memory remember`.".to_string()
    } else {
        record.summary.clone()
    };

    let embed = serenity::CreateEmbed::new()
        .title("🧠 Your Global Memory Profile")
        .description(description)
        .field("Status", status, true)
        .field("Expires", expires, true)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "Last updated: {}",
            record.updated_at
        )))
        .color(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Store a new global memory profile (opt-in)
#[poise::command(slash_command)]
pub async fn remember(
    ctx: Context<'_>,
    #[description = "What should the assistant remember about you?"] summary: String,
    #[description = "Optional: expire after N days"]
    #[min = 1]
    #[max = 3650]
    ttl_days: Option<u64>,
) -> Result<(), Error> {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        ctx.say("❌ Memory summary cannot be empty.").await?;
        return Ok(());
    }

    let service = UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
    service
        .set_user_memory(ctx.author().id.get(), trimmed, ttl_days)
        .await?;

    let expiry_note = match ttl_days {
        Some(days) => format!(" (expires in {} days)", days),
        None => "".to_string(),
    };
    ctx.say(format!(
        "✅ Saved your global memory profile{}.",
        expiry_note
    ))
    .await?;
    Ok(())
}

/// Remove your global memory profile
#[poise::command(slash_command)]
pub async fn forget(ctx: Context<'_>) -> Result<(), Error> {
    let service = UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
    let deleted = service.delete_user_memory(ctx.author().id.get()).await?;

    if deleted == 0 {
        ctx.say("📭 No memory profile to delete.").await?;
    } else {
        ctx.say("✅ Your global memory profile was deleted.")
            .await?;
    }
    Ok(())
}

/// Delete your stored data (messages + memory profile)
#[poise::command(slash_command)]
pub async fn delete_data(ctx: Context<'_>) -> Result<(), Error> {
    info!(
        "User data deletion requested by user {}",
        ctx.author().id.get()
    );

    let confirm_msg = "⚠️ This will delete your stored messages and global memory profile from Mascord. This cannot be undone.";
    let reply = ctx
        .send(
            poise::CreateReply::default()
                .content(confirm_msg)
                .components(vec![serenity::CreateActionRow::Buttons(vec![
                    serenity::CreateButton::new("confirm_user_delete")
                        .label("Delete My Data")
                        .style(serenity::ButtonStyle::Danger),
                    serenity::CreateButton::new("cancel_user_delete")
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
        if interaction.data.custom_id == "confirm_user_delete" {
            let service = UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
            let result = service.purge_user_data(ctx.author().id.get()).await?;
            interaction
                .create_response(
                    ctx.serenity_context(),
                    serenity::CreateInteractionResponse::UpdateMessage(
                        serenity::CreateInteractionResponseMessage::new()
                            .content(format!(
                                "🗑️ Deleted **{}** messages and **{}** memory profile(s). Removed **{}** cached items. \
Cleared **{}** channel summary snapshots and **{}** milestone sets for affected channels.",
                                result.messages_deleted,
                                result.memory_deleted,
                                result.cache_deleted,
                                result.summaries_deleted,
                                result.milestones_deleted
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
                            .content("❌ Deletion cancelled.")
                            .components(vec![]),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}
