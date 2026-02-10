use crate::cache::MessageCache;
use crate::db::{Database, UserMemoryRecord};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};

pub struct UserMemoryService {
    db: Database,
    cache: MessageCache,
}

pub struct UserDataPurgeResult {
    pub messages_deleted: usize,
    pub memory_deleted: usize,
    pub cache_deleted: usize,
    pub summaries_deleted: usize,
    pub milestones_deleted: usize,
}

impl UserMemoryService {
    pub fn new(db: Database, cache: MessageCache) -> Self {
        Self { db, cache }
    }

    pub fn should_skip_memory(text: &str) -> bool {
        let lowered = text.to_lowercase();
        let phrases = [
            "no memory",
            "no-memory",
            "no mem",
            "temporary",
            "temp mode",
            "incognito",
            "do not remember",
            "don't remember",
            "dont remember",
            "do not save",
            "don't save",
            "dont save",
            "do not store",
            "don't store",
            "dont store",
            "forget this",
            "no profile",
        ];
        phrases.iter().any(|p| lowered.contains(p))
    }

    pub fn format_snippet(summary: &str, max_chars: usize) -> String {
        let trimmed = summary.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let mut snippet: String = trimmed.chars().take(max_chars).collect();
        if trimmed.chars().count() > max_chars {
            snippet.push_str("...");
        }

        format!(
            "User memory (short, read-only; use only if relevant): {}",
            snippet
        )
    }

    pub async fn get_user_memory(&self, user_id: u64) -> anyhow::Result<Option<UserMemoryRecord>> {
        let record = self.get_user_memory_record(user_id).await?;
        Ok(record.filter(|r| r.enabled && !r.summary.trim().is_empty()))
    }

    pub async fn get_user_memory_record(
        &self,
        user_id: u64,
    ) -> anyhow::Result<Option<UserMemoryRecord>> {
        let user_id = user_id.to_string();
        let user_id_query = user_id.clone();
        let record = self
            .db
            .run_blocking(move |db| db.get_user_memory(&user_id_query))
            .await?;

        let Some(record) = record else {
            return Ok(None);
        };

        if let Some(expires_at) = record.expires_at.as_deref() {
            if let Some(expires_ts) = parse_sqlite_utc(expires_at) {
                if Utc::now() >= expires_ts {
                    let user_id = user_id.clone();
                    let _ = self
                        .db
                        .run_blocking(move |db| db.delete_user_memory(&user_id))
                        .await;
                    return Ok(None);
                }
            }
        }

        Ok(Some(record))
    }

    pub async fn set_user_memory(
        &self,
        user_id: u64,
        summary: &str,
        ttl_days: Option<u64>,
    ) -> anyhow::Result<()> {
        let expires_at = ttl_days.filter(|d| *d > 0).map(|d| {
            (Utc::now() + Duration::days(d as i64))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        });
        let user_id = user_id.to_string();
        let summary = summary.to_string();
        self.db
            .run_blocking(move |db| db.upsert_user_memory(&user_id, &summary, expires_at))
            .await?;
        Ok(())
    }

    pub async fn set_user_memory_enabled(&self, user_id: u64, enabled: bool) -> anyhow::Result<()> {
        let user_id = user_id.to_string();
        self.db
            .run_blocking(move |db| db.set_user_memory_enabled(&user_id, enabled))
            .await?;
        Ok(())
    }

    pub async fn delete_user_memory(&self, user_id: u64) -> anyhow::Result<usize> {
        let user_id = user_id.to_string();
        self.db
            .run_blocking(move |db| db.delete_user_memory(&user_id))
            .await
    }

    pub async fn cleanup_expired_user_memory(&self) -> anyhow::Result<usize> {
        self.db
            .run_blocking(move |db| db.cleanup_expired_user_memory())
            .await
    }

    pub async fn auto_update_memory(
        &self,
        llm: crate::llm::LlmClient,
        user_id: u64,
        user_message: &str,
        assistant_response: &str,
    ) -> anyhow::Result<Option<String>> {
        let trimmed = user_message.trim();
        if trimmed.len() < 12 {
            return Ok(None);
        }

        if Self::should_skip_memory(trimmed) {
            return Ok(None);
        }

        let record = self.get_user_memory_record(user_id).await?;
        let Some(record) = record else {
            return Ok(None);
        };
        if !record.enabled {
            return Ok(None);
        }

        let current = if record.summary.trim().is_empty() {
            "(none)".to_string()
        } else {
            record.summary.clone()
        };

        let prompt = format!(
            "You maintain a concise global user memory profile. Update it only with durable preferences, \
ongoing projects, or stable facts the user explicitly shared. Do NOT store secrets, credentials, health data, \
financial data, precise location, or sensitive personal data unless the user explicitly asked you to remember it. \
If there is nothing new to add, respond with exactly: NO_UPDATE.\n\n\
CURRENT MEMORY:\n{current}\n\n\
NEW USER MESSAGE:\n{user_message}\n\n\
ASSISTANT RESPONSE (context only):\n{assistant_response}\n\n\
Return updated memory as 1-6 bullet points, max 1200 characters."
        );

        let raw = llm.completion(&prompt).await?;
        let normalized = normalize_memory(&raw, 1200);
        if normalized.is_empty() {
            return Ok(None);
        }

        let expires_at = record.expires_at.clone();
        let user_id_str = user_id.to_string();
        let updated = normalized.clone();
        self.db
            .run_blocking(move |db| db.upsert_user_memory(&user_id_str, &updated, expires_at))
            .await?;

        Ok(Some(normalized))
    }

    pub async fn purge_user_data(&self, user_id: u64) -> anyhow::Result<UserDataPurgeResult> {
        let user_id_str = user_id.to_string();
        let channels = self
            .db
            .run_blocking({
                let user_id_str = user_id_str.clone();
                move |db| db.get_channels_for_user(&user_id_str)
            })
            .await
            .unwrap_or_default();

        let messages_deleted = self
            .db
            .run_blocking({
                let user_id_str = user_id_str.clone();
                move |db| db.purge_messages_by_user(&user_id_str)
            })
            .await?;

        let memory_deleted = self
            .db
            .run_blocking({
                let user_id_str = user_id_str.clone();
                move |db| db.delete_user_memory(&user_id_str)
            })
            .await?;

        let summaries_deleted = if channels.is_empty() {
            0
        } else {
            self.db
                .run_blocking({
                    let channels = channels.clone();
                    move |db| db.delete_channel_summaries(&channels)
                })
                .await
                .unwrap_or(0)
        };

        let milestones_deleted = if channels.is_empty() {
            0
        } else {
            self.db
                .run_blocking({
                    let channels = channels.clone();
                    move |db| db.delete_channel_milestones(&channels)
                })
                .await
                .unwrap_or(0)
        };

        let cache_deleted = self.cache.purge_user_messages(user_id);

        Ok(UserDataPurgeResult {
            messages_deleted,
            memory_deleted,
            cache_deleted,
            summaries_deleted,
            milestones_deleted,
        })
    }
}

fn parse_sqlite_utc(ts: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn normalize_memory(raw: &str, max_chars: usize) -> String {
    let mut text = raw.trim().replace('\r', "");
    if text.is_empty() {
        return String::new();
    }

    let upper = text.to_uppercase();
    if upper.contains("NO_UPDATE") {
        return String::new();
    }

    for prefix in ["UPDATED MEMORY:", "MEMORY:", "UPDATED SUMMARY:", "SUMMARY:"] {
        if upper.starts_with(prefix) {
            if let Some(stripped) = text.get(prefix.len()..) {
                text = stripped.trim().to_string();
            }
            break;
        }
    }

    let mut lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > 6 {
        lines.truncate(6);
    }
    let joined = lines.join("\n");

    truncate_chars(&joined, max_chars)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_memory() {
        assert!(UserMemoryService::should_skip_memory(
            "Please do this in temporary mode"
        ));
        assert!(UserMemoryService::should_skip_memory("no memory this time"));
        assert!(!UserMemoryService::should_skip_memory(
            "Remember that I like concise answers"
        ));
    }

    #[test]
    fn test_normalize_memory() {
        let raw = "UPDATED MEMORY:\n- Prefers Rust\n- Wants concise answers\n";
        let normalized = normalize_memory(raw, 200);
        assert!(normalized.contains("Prefers Rust"));
        assert!(normalized.contains("concise"));

        let no_update = normalize_memory("NO_UPDATE", 200);
        assert!(no_update.is_empty());
    }
}
