use crate::services::reminder::ReminderService;
use crate::{Context, Error};
use chrono::Utc;
use tracing::info;

const MAX_REMINDER_MESSAGE_CHARS: usize = 1500;
const MAX_LIST_RESULTS: usize = 20;
const REMINDER_HELP_TEXT: &str = "\
⏰ Reminder formats:
- `in 2 days, 30 minutes`
- `3 hours`
- `at 5:30PM`
- `at 22:15`
- `2026-02-10 17:30`

Examples:
- `/reminder when:in 2 days, 30 minutes message:Follow up`
- `/reminder when:at 22:15 message:Wrap up tasks`
- `/reminder action:list`
- `/reminder action:cancel reminder_id:42`
- `/reminder action:help`

Clock and absolute date/time inputs are interpreted as UTC.";

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum ReminderAction {
    #[name = "list"]
    List,
    #[name = "cancel"]
    Cancel,
    #[name = "help"]
    Help,
}

/// Create reminders directly, or manage reminders via `action`
#[poise::command(slash_command, guild_only)]
pub async fn reminder(
    ctx: Context<'_>,
    #[description = "When to remind (e.g., in 2 days, 30 minutes, at 22:15)"] when: Option<String>,
    #[description = "Reminder message"] message: Option<String>,
    #[description = "Optional management action"] action: Option<ReminderAction>,
    #[description = "Reminder ID (required for action=cancel)"] reminder_id: Option<i64>,
    #[description = "Max reminders to show for action=list (default 10)"]
    #[min = 1]
    #[max = 20]
    limit: Option<u8>,
) -> Result<(), Error> {
    match action {
        Some(ReminderAction::List) => {
            if when.is_some() || message.is_some() || reminder_id.is_some() {
                ctx.say("❌ For `action=list`, only `limit` is allowed.")
                    .await?;
                return Ok(());
            }
            list_reminders(ctx, limit).await
        }
        Some(ReminderAction::Cancel) => {
            if when.is_some() || message.is_some() || limit.is_some() {
                ctx.say("❌ For `action=cancel`, only `reminder_id` is allowed.")
                    .await?;
                return Ok(());
            }
            let reminder_id = match reminder_id {
                Some(id) => id,
                None => {
                    ctx.say("❌ `reminder_id` is required when `action=cancel`.")
                        .await?;
                    return Ok(());
                }
            };
            cancel_reminder(ctx, reminder_id).await
        }
        Some(ReminderAction::Help) => {
            if when.is_some() || message.is_some() || reminder_id.is_some() || limit.is_some() {
                ctx.say("❌ For `action=help`, no other options are needed.")
                    .await?;
                return Ok(());
            }
            help(ctx).await
        }
        None => {
            if reminder_id.is_some() || limit.is_some() {
                ctx.say("❌ `reminder_id`/`limit` require an `action`.")
                    .await?;
                return Ok(());
            }

            let when = match when {
                Some(value) if !value.trim().is_empty() => value,
                _ => {
                    ctx.say(format!(
                        "❌ Provide `when` and `message` to set a reminder.\n\n{REMINDER_HELP_TEXT}"
                    ))
                    .await?;
                    return Ok(());
                }
            };
            let message = match message {
                Some(value) => value,
                None => {
                    ctx.say("❌ `message` is required when setting a reminder.")
                        .await?;
                    return Ok(());
                }
            };

            set_reminder(ctx, when, message).await
        }
    }
}

async fn set_reminder(ctx: Context<'_>, when: String, message: String) -> Result<(), Error> {
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

    let remind_at = match ReminderService::parse_schedule_input(when.trim(), Utc::now()) {
        Ok(remind_at) => remind_at,
        Err(err) => {
            ctx.say(format!("❌ {err}\n\n{REMINDER_HELP_TEXT}")).await?;
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

/// Show reminder usage and supported time formats
async fn help(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(REMINDER_HELP_TEXT).await?;
    Ok(())
}

/// List your upcoming reminders
async fn list_reminders(ctx: Context<'_>, limit: Option<u8>) -> Result<(), Error> {
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
async fn cancel_reminder(ctx: Context<'_>, reminder_id: i64) -> Result<(), Error> {
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
