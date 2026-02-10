use crate::reminder::{ReminderService, REMINDER_MAX_MINUTES};
use crate::{Context, Error};
use poise::serenity_prelude as serenity;
use tracing::info;

const LIST_LIMIT: usize = 20;
const PREVIEW_LIMIT: usize = 90;

/// Manage personal reminders
#[poise::command(
    slash_command,
    subcommands("set", "list", "cancel", "help"),
    guild_only
)]
pub async fn remind(ctx: Context<'_>) -> Result<(), Error> {
    send_remind_help(&ctx).await?;
    Ok(())
}

/// Show reminder usage help and examples
#[poise::command(slash_command, guild_only)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    send_remind_help(&ctx).await?;
    Ok(())
}

/// Set a reminder using natural-language time input
#[poise::command(slash_command, guild_only)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "When to remind (e.g. 'in 2 days, 30 minutes', '3 hours', 'at 22:15')"]
    when: String,
    #[description = "Reminder text"] message: String,
) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let service = ReminderService::new(ctx.data().db.clone());

    info!(
        "Remind set requested by {} in guild {} (schedule: {})",
        ctx.author().id,
        guild_id,
        when
    );

    let created = service.create_from_schedule_input(
        &guild_id.to_string(),
        &ctx.channel_id().to_string(),
        &ctx.author().id.to_string(),
        &when,
        &message,
    )?;

    let ts = created.remind_at.timestamp();
    let display_when = when.replace('`', "'");
    ctx.send(
        poise::CreateReply::default()
            .ephemeral(true)
            .content(format!(
                "⏰ Reminder `#{}` set for <t:{}:F> (<t:{}:R>).\nSchedule input: `{}`\nMax delay is {} minutes. Clock-style times are interpreted in UTC. Use `/remind help` for examples.",
                created.id, ts, ts, display_when, REMINDER_MAX_MINUTES
            )),
    )
    .await?;

    Ok(())
}

/// List your pending reminders
#[poise::command(slash_command, guild_only)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let service = ReminderService::new(ctx.data().db.clone());

    let reminders = service.list_pending_for_user(
        &guild_id.to_string(),
        &ctx.author().id.to_string(),
        LIST_LIMIT,
    )?;

    if reminders.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("📭 You have no pending reminders in this server."),
        )
        .await?;
        return Ok(());
    }

    let mut description = String::new();
    for reminder in reminders {
        let ts = reminder.remind_at.timestamp();
        description.push_str(&format!(
            "`#{}` • <#{}> • <t:{}:F> (<t:{}:R>)\n{}\n\n",
            reminder.id,
            reminder.channel_id,
            ts,
            ts,
            truncate(&reminder.message, PREVIEW_LIMIT)
        ));
    }

    let embed = serenity::CreateEmbed::new()
        .title("⏰ Your Pending Reminders")
        .description(description.trim_end())
        .footer(serenity::CreateEmbedFooter::new(
            "Use /remind cancel <id> to cancel a reminder",
        ))
        .color(0xFEE75C);

    ctx.send(poise::CreateReply::default().ephemeral(true).embed(embed))
        .await?;
    Ok(())
}

/// Cancel one of your pending reminders
#[poise::command(slash_command, guild_only)]
pub async fn cancel(
    ctx: Context<'_>,
    #[description = "Reminder ID (from /remind list)"] reminder_id: i64,
) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let service = ReminderService::new(ctx.data().db.clone());

    let cancelled = service.cancel_for_user(
        &guild_id.to_string(),
        &ctx.author().id.to_string(),
        reminder_id,
    )?;

    let content = if cancelled {
        format!("✅ Cancelled reminder `#{}`.", reminder_id)
    } else {
        format!(
            "ℹ️ Reminder `#{}` was not found or is already processed.",
            reminder_id
        )
    };

    ctx.send(
        poise::CreateReply::default()
            .ephemeral(true)
            .content(content),
    )
    .await?;

    Ok(())
}

fn truncate(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

async fn send_remind_help(ctx: &Context<'_>) -> Result<(), Error> {
    let description = "\
**Set reminders**\n\
`/remind set <when> <message>`\n\n\
**Accepted `when` formats**\n\
- `in 2 days, 30 minutes`\n\
- `3 hours`\n\
- `at 5:30PM`\n\
- `at 22:15`\n\
- `2026-02-10 17:30` (UTC)\n\n\
**Notes**\n\
- Numeric-only inputs like `10` are not accepted; include a unit like `10 minutes`.\n\
- Clock-style and absolute datetime inputs are interpreted as UTC.\n\
- Delay range: 10 seconds to 30 days.";

    let embed = serenity::CreateEmbed::new()
        .title("⏰ Reminder Help")
        .description(description)
        .color(0xFEE75C);

    ctx.send(poise::CreateReply::default().ephemeral(true).embed(embed))
        .await?;
    Ok(())
}
