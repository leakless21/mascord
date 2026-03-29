use crate::services::reminder::ReminderService;
use crate::services::reminder_ops::{self, SetReminderOp, REMINDER_HELP_TEXT};
use crate::{Context, Error};
use chrono::Utc;
use tracing::info;

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
                    ctx.say(SetReminderOp::EmptyWhen.discord_message()).await?;
                    return Ok(());
                }
            };
            let message = match message {
                Some(value) => value,
                None => {
                    ctx.say(SetReminderOp::EmptyMessage.discord_message())
                        .await?;
                    return Ok(());
                }
            };

            set_reminder(ctx, when, message).await
        }
    }
}

async fn set_reminder(ctx: Context<'_>, when: String, message: String) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?;
    let channel_id = ctx.channel_id();
    let user_id = ctx.author().id;

    let service = ReminderService::new(ctx.data().db.clone());
    let op = reminder_ops::set_reminder(
        &service,
        guild_id.get(),
        channel_id.get(),
        user_id.get(),
        &when,
        &message,
        Utc::now(),
    )
    .await?;

    if let SetReminderOp::Created {
        reminder_id,
        remind_at,
    } = &op
    {
        info!(
            "Created reminder {} for user {} in channel {} at {}",
            reminder_id, user_id, channel_id, remind_at
        );
    }

    ctx.say(op.discord_message()).await?;
    Ok(())
}

async fn help(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(REMINDER_HELP_TEXT).await?;
    Ok(())
}

async fn list_reminders(ctx: Context<'_>, limit: Option<u8>) -> Result<(), Error> {
    let limit = reminder_ops::clamp_list_limit_option_u8(limit);
    let service = ReminderService::new(ctx.data().db.clone());
    let items = reminder_ops::list_reminders(&service, ctx.author().id.get(), limit).await?;
    ctx.say(reminder_ops::format_reminder_list_discord(&items))
        .await?;
    Ok(())
}

async fn cancel_reminder(ctx: Context<'_>, reminder_id: i64) -> Result<(), Error> {
    let service = ReminderService::new(ctx.data().db.clone());
    let op = reminder_ops::cancel_reminder(&service, reminder_id, ctx.author().id.get()).await?;
    ctx.say(op.discord_message()).await?;
    Ok(())
}
