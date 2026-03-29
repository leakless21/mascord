//! One agent tool [`ReminderTool`] named `reminder` — same capabilities as `/reminder` via `action` (no duplicate tools per subcommand).

use crate::db::Database;
use crate::llm::confirm::DiscordToolContext;
use crate::services::reminder::ReminderService;
use crate::services::reminder_ops::{self, REMINDER_AGENT_TOOL_NAME, REMINDER_HELP_TEXT};
use crate::tools::Tool;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

pub struct ReminderTool;

fn parse_action(params: &Value) -> Option<&str> {
    params
        .get("action")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Core routing for the `reminder` tool (no Serenity); used by [`ReminderTool`] and unit tests.
pub async fn dispatch_reminder_tool(
    params: Value,
    guild_id: Option<u64>,
    channel_id: u64,
    user_id: u64,
    db: Database,
) -> anyhow::Result<Value> {
    let action = match parse_action(&params) {
        Some(a) => a.to_ascii_lowercase(),
        None => {
            return Ok(json!({
                "status": "error",
                "message": "Missing or empty `action`. Use set, list, cancel, or help."
            }));
        }
    };

    let service = ReminderService::new(db);

    match action.as_str() {
        "help" => Ok(json!({
            "status": "ok",
            "help": REMINDER_HELP_TEXT
        })),
        "set" => {
            let Some(guild_id) = guild_id else {
                return Ok(json!({
                    "status": "error",
                    "message": "action=set requires a server (guild) channel, not DMs."
                }));
            };
            let when = params
                .get("when")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let op = reminder_ops::set_reminder(
                &service,
                guild_id,
                channel_id,
                user_id,
                when,
                message,
                Utc::now(),
            )
            .await?;
            Ok(op.to_tool_json())
        }
        "list" => {
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(10);
            let limit = reminder_ops::clamp_list_limit_usize(limit);
            let items = reminder_ops::list_reminders(&service, user_id, limit).await?;
            if items.is_empty() {
                return Ok(json!({"status": "ok", "reminders": []}));
            }
            let reminders: Vec<Value> = items.iter().map(|i| i.to_tool_json()).collect();
            Ok(json!({"status": "ok", "reminders": reminders}))
        }
        "cancel" => {
            let reminder_id = match params.get("reminder_id").and_then(|v| v.as_i64()) {
                Some(id) => id,
                None => {
                    return Ok(json!({
                        "status": "error",
                        "message": "action=cancel requires `reminder_id`."
                    }));
                }
            };
            let op = reminder_ops::cancel_reminder(&service, reminder_id, user_id).await?;
            Ok(op.to_tool_json())
        }
        _ => Ok(json!({
            "status": "error",
            "message": format!(
                "Unknown action `{action}`. Use set, list, cancel, or help."
            )
        })),
    }
}

#[async_trait]
impl Tool for ReminderTool {
    fn name(&self) -> &str {
        REMINDER_AGENT_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Same as the /reminder slash command: manage reminders with `action` set | list | cancel | help. For action=set provide `when` and `message` (natural-language time, UTC). For list optional `limit` (1–20). For cancel provide `reminder_id`. Setting reminders requires a server channel (guild), not DMs."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["set", "list", "cancel", "help"],
                    "description": "set | list | cancel | help — mirrors /reminder"
                },
                "when": {
                    "type": "string",
                    "description": "Required for action=set (e.g. in 2 hours, at 22:15)"
                },
                "message": {
                    "type": "string",
                    "description": "Required for action=set"
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional for action=list (default 10, max 20)"
                },
                "reminder_id": {
                    "type": "integer",
                    "description": "Required for action=cancel"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, _params: Value) -> anyhow::Result<Value> {
        Ok(json!({
            "status": "error",
            "message": "reminder requires Discord context; mention the bot or reply in a channel."
        }))
    }

    async fn execute_with_discord(
        &self,
        params: Value,
        dctx: Option<&DiscordToolContext<'_>>,
    ) -> anyhow::Result<Value> {
        let Some(ctx) = dctx else {
            return self.execute(params).await;
        };
        dispatch_reminder_tool(
            params,
            ctx.guild_id.map(|g| g.get()),
            ctx.channel_id.get(),
            ctx.user_id.get(),
            ctx.data.db.clone(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_memory_config;
    use crate::db::Database;
    use crate::tools::Tool;
    use serde_json::json;

    #[test]
    fn tool_name_and_schema() {
        let t = ReminderTool;
        assert_eq!(t.name(), REMINDER_AGENT_TOOL_NAME);
        let schema = t.parameters_schema();
        let req = schema["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "action"));
    }

    #[test]
    fn parse_action_trims_and_rejects_empty() {
        assert!(parse_action(&json!({})).is_none());
        assert!(parse_action(&json!({"action": ""})).is_none());
        assert!(parse_action(&json!({"action": "   "})).is_none());
        assert_eq!(parse_action(&json!({"action": " list "})), Some("list"));
    }

    #[tokio::test]
    async fn execute_without_discord_returns_error_status() {
        let t = ReminderTool;
        let r = t.execute(json!({"action": "help"})).await.unwrap();
        assert_eq!(r["status"], "error");
    }

    #[tokio::test]
    async fn dispatch_help_needs_no_guild() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let r = dispatch_reminder_tool(json!({"action": "help"}), None, 100, 1, db)
            .await
            .unwrap();
        assert_eq!(r["status"], "ok");
        assert!(r["help"].as_str().unwrap().contains("/reminder"));
    }

    #[tokio::test]
    async fn dispatch_set_requires_guild() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let r = dispatch_reminder_tool(
            json!({"action": "set", "when": "in 2 hours", "message": "hi"}),
            None,
            10,
            1,
            db,
        )
        .await
        .unwrap();
        assert_eq!(r["status"], "error");
        assert!(r["message"].as_str().unwrap().contains("guild"));
    }

    #[tokio::test]
    async fn dispatch_set_list_cancel_roundtrip() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let guild = 9001u64;
        let channel = 200u64;
        let user = 42u64;

        let set = dispatch_reminder_tool(
            json!({"action": "set", "when": "in 2 hours", "message": "roundtrip test"}),
            Some(guild),
            channel,
            user,
            db.clone(),
        )
        .await
        .unwrap();
        assert_eq!(set["status"], "ok");
        let rid = set["reminder_id"].as_i64().unwrap();

        let list = dispatch_reminder_tool(
            json!({"action": "list", "limit": 5}),
            None,
            channel,
            user,
            db.clone(),
        )
        .await
        .unwrap();
        assert_eq!(list["status"], "ok");
        let arr = list["reminders"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], rid);

        let cancel = dispatch_reminder_tool(
            json!({"action": "cancel", "reminder_id": rid}),
            None,
            channel,
            user,
            db.clone(),
        )
        .await
        .unwrap();
        assert_eq!(cancel["status"], "ok");

        let list2 = dispatch_reminder_tool(json!({"action": "list"}), None, channel, user, db)
            .await
            .unwrap();
        assert!(list2["reminders"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_cancel_wrong_user_not_found() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let set = dispatch_reminder_tool(
            json!({"action": "set", "when": "in 3 hours", "message": "mine"}),
            Some(1),
            2,
            100,
            db.clone(),
        )
        .await
        .unwrap();
        let rid = set["reminder_id"].as_i64().unwrap();

        let cancel = dispatch_reminder_tool(
            json!({"action": "cancel", "reminder_id": rid}),
            None,
            2,
            999,
            db,
        )
        .await
        .unwrap();
        assert_eq!(cancel["status"], "error");
    }

    #[tokio::test]
    async fn dispatch_unknown_action() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let r = dispatch_reminder_tool(json!({"action": "nope"}), Some(1), 1, 1, db)
            .await
            .unwrap();
        assert_eq!(r["status"], "error");
        assert!(r["message"].as_str().unwrap().contains("Unknown"));
    }

    #[tokio::test]
    async fn dispatch_cancel_missing_id() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let r = dispatch_reminder_tool(json!({"action": "cancel"}), None, 1, 1, db)
            .await
            .unwrap();
        assert_eq!(r["status"], "error");
    }

    #[tokio::test]
    async fn dispatch_list_clamps_high_limit() {
        let db = Database::new(&test_memory_config()).unwrap();
        db.execute_init().unwrap();
        let r = dispatch_reminder_tool(json!({"action": "list", "limit": 999}), None, 1, 1, db)
            .await
            .unwrap();
        assert_eq!(r["status"], "ok");
    }
}
