use crate::db::Database;
use crate::tools::Tool;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{json, Value};

pub struct GetUserMemoryTool {
    pub db: Database,
}

#[async_trait]
impl Tool for GetUserMemoryTool {
    fn name(&self) -> &str {
        "get_user_memory"
    }

    fn description(&self) -> &str {
        "Fetch the user's full global memory profile (opt-in). Use when detailed preferences or background are needed. Requires user_id."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "user_id": {
                    "type": "string",
                    "description": "Discord user id for the profile to fetch"
                }
            },
            "required": ["user_id"]
        })
    }

    async fn execute(&self, params: Value) -> anyhow::Result<Value> {
        let user_id = params["user_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing user_id"))?
            .to_string();

        let record = self
            .db
            .run_blocking({
                let user_id = user_id.clone();
                move |db| db.get_user_memory(&user_id)
            })
            .await?;

        let Some(record) = record else {
            return Ok(json!({"result": "No user memory profile found."}));
        };

        if !record.enabled || record.summary.trim().is_empty() {
            return Ok(json!({"result": "User memory is disabled or empty."}));
        }

        if let Some(expires_at) = record.expires_at.as_deref() {
            if let Some(expires_ts) = parse_sqlite_utc(expires_at) {
                if Utc::now() >= expires_ts {
                    let _ = self
                        .db
                        .run_blocking(move |db| db.delete_user_memory(&user_id))
                        .await;
                    return Ok(json!({"result": "User memory profile expired."}));
                }
            }
        }

        Ok(json!({
            "result": record.summary,
            "updated_at": record.updated_at,
            "expires_at": record.expires_at
        }))
    }
}

fn parse_sqlite_utc(ts: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}
