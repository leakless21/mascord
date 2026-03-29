//! Unified reminder operations: one task list for `/reminder`, the single LLM tool `reminder`, and formatting.
//!
//! Add new reminder capabilities here first, then wire the slash command and [`crate::tools::builtin::reminder::ReminderTool`].

use crate::db::ReminderRecord;
use crate::services::reminder::{
    ReminderScheduleError, ReminderService, MAX_LIST_RESULTS, MAX_REMINDER_MESSAGE_CHARS,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

/// User-facing help (slash `action=help` and validation errors).
pub const REMINDER_HELP_TEXT: &str = "\
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

/// Canonical operations (slash `action` modes and agent `action` string — same surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderTask {
    Set,
    List,
    Cancel,
    Help,
}

/// Single LLM function name for reminders; mirrors `/reminder` (do not add `set_reminder` / `list_reminders` duplicates).
pub const REMINDER_AGENT_TOOL_NAME: &str = "reminder";

impl ReminderTask {
    pub const ALL: &[Self] = &[Self::Set, Self::List, Self::Cancel, Self::Help];

    pub fn slash_hint(self) -> &'static str {
        match self {
            ReminderTask::Set => "when + message",
            ReminderTask::List => "action=list",
            ReminderTask::Cancel => "action=cancel + reminder_id",
            ReminderTask::Help => "action=help",
        }
    }
}

pub fn truncate_snippet(message: &str, max_chars: usize) -> String {
    let mut snippet: String = message.chars().take(max_chars).collect();
    if message.chars().count() > max_chars {
        snippet.push_str("...");
    }
    snippet
}

pub fn clamp_list_limit_option_u8(limit: Option<u8>) -> usize {
    limit
        .map(|v| v as usize)
        .unwrap_or(10)
        .min(MAX_LIST_RESULTS)
}

pub fn clamp_list_limit_usize(limit: usize) -> usize {
    limit.clamp(1, MAX_LIST_RESULTS)
}

#[derive(Debug, Clone)]
pub struct ReminderListItem {
    pub id: i64,
    pub remind_at: DateTime<Utc>,
    pub channel_id: String,
    pub message_snippet: String,
}

impl ReminderListItem {
    pub fn from_record(r: ReminderRecord) -> Self {
        let remind_at = ReminderService::parse_sqlite_utc(&r.remind_at).unwrap_or_else(Utc::now);
        let message_snippet = truncate_snippet(&r.message, 80);
        Self {
            id: r.id,
            remind_at,
            channel_id: r.channel_id,
            message_snippet,
        }
    }

    pub fn to_tool_json(&self) -> Value {
        json!({
            "id": self.id,
            "relative_timestamp": self.remind_at.timestamp(),
            "channel_id": self.channel_id,
            "message_snippet": self.message_snippet,
        })
    }

    /// Line for slash / list display (Discord markdown).
    pub fn to_discord_line(&self) -> String {
        let when = format!("<t:{}:R>", self.remind_at.timestamp());
        let channel = format!("<#{}>", self.channel_id);
        format!(
            "• `{}` {} in {} — {}",
            self.id, when, channel, self.message_snippet
        )
    }
}

#[derive(Debug)]
pub enum SetReminderOp {
    Created {
        reminder_id: i64,
        remind_at: DateTime<Utc>,
    },
    EmptyWhen,
    EmptyMessage,
    MessageTooLong,
    BadSchedule(ReminderScheduleError),
}

impl SetReminderOp {
    pub fn discord_message(&self) -> String {
        match self {
            SetReminderOp::Created {
                reminder_id,
                remind_at,
            } => {
                let unix = remind_at.timestamp();
                format!("✅ Reminder set for <t:{unix}:F> (<t:{unix}:R>). ID: `{reminder_id}`")
            }
            SetReminderOp::EmptyWhen => format!(
                "❌ Provide `when` and `message` to set a reminder.\n\n{REMINDER_HELP_TEXT}"
            ),
            SetReminderOp::EmptyMessage => {
                "❌ `message` is required when setting a reminder.".to_string()
            }
            SetReminderOp::MessageTooLong => format!(
                "❌ Reminder message is too long (max {} characters).",
                MAX_REMINDER_MESSAGE_CHARS
            ),
            SetReminderOp::BadSchedule(e) => format!("❌ {e}\n\n{REMINDER_HELP_TEXT}"),
        }
    }

    pub fn to_tool_json(&self) -> Value {
        match self {
            SetReminderOp::Created {
                reminder_id,
                remind_at,
            } => json!({
                "status": "ok",
                "reminder_id": reminder_id,
                "remind_at_unix": remind_at.timestamp(),
                "when_parsed_utc": remind_at.to_rfc3339(),
            }),
            SetReminderOp::EmptyWhen => json!({
                "status": "error",
                "message": "`when` is required."
            }),
            SetReminderOp::EmptyMessage => json!({
                "status": "error",
                "message": "`message` cannot be empty."
            }),
            SetReminderOp::MessageTooLong => json!({
                "status": "error",
                "message": format!("Message too long (max {MAX_REMINDER_MESSAGE_CHARS} characters).")
            }),
            SetReminderOp::BadSchedule(e) => {
                json!({"status": "error", "message": e.to_string()})
            }
        }
    }
}

/// Set a reminder (guild channel). DB errors propagate as `Err`.
pub async fn set_reminder(
    svc: &ReminderService,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
    when: &str,
    message: &str,
    now: DateTime<Utc>,
) -> Result<SetReminderOp, anyhow::Error> {
    let when = when.trim();
    let message = message.trim();

    if when.is_empty() {
        return Ok(SetReminderOp::EmptyWhen);
    }
    if message.is_empty() {
        return Ok(SetReminderOp::EmptyMessage);
    }
    if message.chars().count() > MAX_REMINDER_MESSAGE_CHARS {
        return Ok(SetReminderOp::MessageTooLong);
    }

    let remind_at = match ReminderService::parse_schedule_input(when, now) {
        Ok(t) => t,
        Err(e) => return Ok(SetReminderOp::BadSchedule(e)),
    };

    let reminder_id = svc
        .create_reminder(guild_id, channel_id, user_id, message, remind_at)
        .await?;

    Ok(SetReminderOp::Created {
        reminder_id,
        remind_at,
    })
}

pub async fn list_reminders(
    svc: &ReminderService,
    user_id: u64,
    limit: usize,
) -> anyhow::Result<Vec<ReminderListItem>> {
    let limit = clamp_list_limit_usize(limit);
    let rows = svc.list_pending_reminders(user_id, limit).await?;
    Ok(rows
        .into_iter()
        .map(ReminderListItem::from_record)
        .collect())
}

#[derive(Debug)]
pub enum CancelReminderOp {
    Cancelled { reminder_id: i64 },
    NotFound,
}

impl CancelReminderOp {
    pub fn discord_message(&self) -> String {
        match self {
            CancelReminderOp::Cancelled { reminder_id } => {
                format!("✅ Reminder `{reminder_id}` cancelled.")
            }
            CancelReminderOp::NotFound => "❌ No pending reminder found with that ID.".to_string(),
        }
    }

    pub fn to_tool_json(&self) -> Value {
        match self {
            CancelReminderOp::Cancelled { reminder_id } => {
                json!({"status": "ok", "cancelled_id": reminder_id})
            }
            CancelReminderOp::NotFound => json!({
                "status": "error",
                "message": "No pending reminder with that ID for this user."
            }),
        }
    }
}

pub async fn cancel_reminder(
    svc: &ReminderService,
    reminder_id: i64,
    user_id: u64,
) -> Result<CancelReminderOp, anyhow::Error> {
    let deleted = svc.delete_pending_reminder(reminder_id, user_id).await?;
    if deleted == 0 {
        return Ok(CancelReminderOp::NotFound);
    }
    Ok(CancelReminderOp::Cancelled { reminder_id })
}

pub fn format_reminder_list_discord(items: &[ReminderListItem]) -> String {
    if items.is_empty() {
        return "📭 No upcoming reminders.".to_string();
    }
    let lines: Vec<String> = items.iter().map(|i| i.to_discord_line()).collect();
    format!("**Your upcoming reminders:**\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_memory_config;
    use crate::db::Database;
    use crate::services::reminder::ReminderService;

    #[test]
    fn task_list_covers_all_slash_modes() {
        assert_eq!(ReminderTask::ALL.len(), 4);
    }

    #[test]
    fn agent_uses_single_tool_matching_slash_command() {
        assert_eq!(REMINDER_AGENT_TOOL_NAME, "reminder");
    }

    #[test]
    fn clamp_list_limit_usize_bounds() {
        assert_eq!(clamp_list_limit_usize(0), 1);
        assert_eq!(clamp_list_limit_usize(1), 1);
        assert_eq!(clamp_list_limit_usize(10), 10);
        assert_eq!(clamp_list_limit_usize(20), 20);
        assert_eq!(clamp_list_limit_usize(999), MAX_LIST_RESULTS);
    }

    #[test]
    fn clamp_list_limit_option_u8_variants() {
        assert_eq!(super::clamp_list_limit_option_u8(None), 10);
        assert_eq!(super::clamp_list_limit_option_u8(Some(1)), 1);
        assert_eq!(super::clamp_list_limit_option_u8(Some(20)), 20);
        assert_eq!(
            super::clamp_list_limit_option_u8(Some(99)),
            MAX_LIST_RESULTS
        );
    }

    #[test]
    fn truncate_snippet_adds_ellipsis() {
        let s = "a".repeat(100);
        let out = truncate_snippet(&s, 10);
        assert_eq!(out.len(), 13);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn format_reminder_list_discord_empty() {
        assert_eq!(
            format_reminder_list_discord(&[]),
            "📭 No upcoming reminders."
        );
    }

    #[test]
    fn set_reminder_op_tool_json_created_shape() {
        let remind_at = Utc::now();
        let op = SetReminderOp::Created {
            reminder_id: 7,
            remind_at,
        };
        let j = op.to_tool_json();
        assert_eq!(j["status"], "ok");
        assert_eq!(j["reminder_id"], 7);
        assert_eq!(j["remind_at_unix"].as_i64().unwrap(), remind_at.timestamp());
    }

    #[tokio::test]
    async fn ops_set_validation_empty_when() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let svc = ReminderService::new(db);
        let op = set_reminder(&svc, 1, 2, 3, "", "msg", Utc::now())
            .await
            .unwrap();
        assert!(matches!(op, SetReminderOp::EmptyWhen));
    }

    #[tokio::test]
    async fn ops_set_validation_message_too_long() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let svc = ReminderService::new(db);
        let msg = "x".repeat(MAX_REMINDER_MESSAGE_CHARS + 1);
        let op = set_reminder(&svc, 1, 2, 3, "in 2 hours", &msg, Utc::now())
            .await
            .unwrap();
        assert!(matches!(op, SetReminderOp::MessageTooLong));
    }

    #[tokio::test]
    async fn ops_full_flow_via_service_layer() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let svc = ReminderService::new(db);
        let g = 10u64;
        let c = 20u64;
        let u = 30u64;

        let created = set_reminder(&svc, g, c, u, "in 2 hours", "ops flow", Utc::now())
            .await
            .unwrap();
        let SetReminderOp::Created { reminder_id, .. } = created else {
            panic!("expected Created");
        };

        let items = list_reminders(&svc, u, 10).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, reminder_id);

        let cancel = cancel_reminder(&svc, reminder_id, u).await.unwrap();
        assert!(matches!(
            cancel,
            CancelReminderOp::Cancelled {
                reminder_id: id
            } if id == reminder_id
        ));

        let again = cancel_reminder(&svc, reminder_id, u).await.unwrap();
        assert!(matches!(again, CancelReminderOp::NotFound));
    }
}
