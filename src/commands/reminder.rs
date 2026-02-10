use crate::services::reminder::ReminderService;
use crate::{Context, Error};
use chrono::{Duration as ChronoDuration, Utc};
use humantime::parse_duration;
use tracing::info;

const MAX_REMINDER_MESSAGE_CHARS: usize = 1500;
const MAX_LIST_RESULTS: usize = 20;
const MIN_REMINDER_SECS: u64 = 60;

/// Manage reminders
#[poise::command(slash_command, subcommands("set", "list", "cancel"), guild_only)]
pub async fn reminder(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Set a reminder (duration examples: 10m, 2h, 1d 2h)
#[poise::command(slash_command, guild_only)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "When to remind you (e.g., 10m, 2h, 1d 2h)"] when: String,
    #[description = "Reminder message"] message: String,
) -> Result<(), Error> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        ctx.say("❌ Reminder message cannot be empty.").await?;
        return Ok(());
    }
    if trimmed.chars().count() > MAX_REMINDER_MESSAGE_CHARS {
        ctx.say(format!(
            "❌ Reminder message is too long (max {} characters).",
            MAX_REMINDER_MESSAGE_CHARS
        ))
        .await?;
        return Ok(());
    }

    let duration = match parse_duration(when.trim()) {
        Ok(duration) => duration,
        Err(_) => {
            ctx.say("❌ Invalid duration. Examples: `10m`, `2h`, `1d 2h`.")
                .await?;
            return Ok(());
        }
    };

    if duration.as_secs() < MIN_REMINDER_SECS {
        ctx.say("❌ Reminders must be at least 1 minute in the future.")
            .await?;
        return Ok(());
    }

    let remind_at = match ChronoDuration::from_std(duration) {
        Ok(delta) => Utc::now() + delta,
        Err(_) => {
            ctx.say("❌ Reminder duration is too large.").await?;
            return Ok(());
        }
    };

    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    let channel_id = ctx.channel_id();
    let user_id = ctx.author().id;

    let service = ReminderService::new(ctx.data().db.clone());
    let reminder_id = service
        .create_reminder(
            guild_id.get(),
            channel_id.get(),
            user_id.get(),
            trimmed,
            remind_at,
        )
        .await?;

    let unix = remind_at.timestamp();
    info!(
        "Created reminder {} for user {} in channel {} at {}",
        reminder_id, user_id, channel_id, remind_at
    );

    ctx.say(format!(
        "✅ Reminder set for <t:{unix}:F> (<t:{unix}:R>). ID: `{reminder_id}`"
    ))
    .await?;
    Ok(())
}

/// List your upcoming reminders
#[poise::command(slash_command, guild_only)]
pub async fn list(
    ctx: Context<'_>,
    #[description = "Max reminders to show (default 10)"]
    #[min = 1]
    #[max = 20]
    limit: Option<u8>,
) -> Result<(), Error> {
    let limit = limit
        .map(|v| v as usize)
        .unwrap_or(10)
        .min(MAX_LIST_RESULTS);
    let service = ReminderService::new(ctx.data().db.clone());
    let reminders = service
        .list_pending_reminders(ctx.author().id.get(), limit)
        .await?;

    if reminders.is_empty() {
        ctx.say("📭 No upcoming reminders.").await?;
        return Ok(());
    }

    let mut lines = Vec::new();
    for reminder in reminders {
        let when = ReminderService::parse_sqlite_utc(&reminder.remind_at)
            .map(|dt| format!("<t:{}:R>", dt.timestamp()))
            .unwrap_or_else(|| reminder.remind_at.clone());
        let channel = format!("<#{}>", reminder.channel_id);
        let snippet = truncate_message(&reminder.message, 80);
        lines.push(format!(
            "• `{}` {} in {} — {}",
            reminder.id, when, channel, snippet
        ));
    }

    let response = format!("**Your upcoming reminders:**\n{}", lines.join("\n"));
    ctx.say(response).await?;
    Ok(())
}

/// Cancel a pending reminder
#[poise::command(slash_command, guild_only)]
pub async fn cancel(
    ctx: Context<'_>,
    #[description = "Reminder ID to cancel"] reminder_id: i64,
) -> Result<(), Error> {
    let service = ReminderService::new(ctx.data().db.clone());
    let deleted = service
        .delete_pending_reminder(reminder_id, ctx.author().id.get())
        .await?;

    if deleted == 0 {
        ctx.say("❌ No pending reminder found with that ID.")
            .await?;
        return Ok(());
    }

    ctx.say(format!("✅ Reminder `{}` cancelled.", reminder_id))
        .await?;
    Ok(())
}

fn truncate_message(message: &str, max_chars: usize) -> String {
    let mut snippet: String = message.chars().take(max_chars).collect();
    if message.chars().count() > max_chars {
        snippet.push_str("...");
    }
    snippet
}
