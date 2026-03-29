//! Background consolidation of stored memory (AutoDream): dedupe, drop stale lines, shrink noise.
//! Runs on a timer; respects [`crate::config::Config`] limits and optional job leases.

use crate::cache::MessageCache;
use crate::config::Config;
use crate::db::Database;
use crate::llm::LlmClient;
use crate::services::user_memory::normalize_memory;
use crate::summarize::extract_milestones_for_summary;
use chrono::{Duration, Utc};
use tracing::{info, warn};

#[derive(Clone)]
pub struct AutoDreamPolicy {
    pub min_hours_between: i64,
    pub max_users_per_cycle: usize,
    pub channel_summaries: bool,
    pub max_channels_per_cycle: usize,
    pub user_max_chars: usize,
    pub channel_max_chars: usize,
    pub activity_gate_hours: u64,
    pub activity_min_messages: usize,
    pub channel_activity_hours: Option<u64>,
}

impl AutoDreamPolicy {
    pub fn from_config(config: &Config) -> Self {
        let channel_activity_hours = if config.autodream_channel_activity_hours > 0 {
            Some(config.autodream_channel_activity_hours)
        } else {
            None
        };
        Self {
            min_hours_between: config.autodream_min_hours,
            max_users_per_cycle: config.autodream_max_users_per_cycle,
            channel_summaries: config.autodream_channel_summaries,
            max_channels_per_cycle: config.autodream_max_channels_per_cycle,
            user_max_chars: config.autodream_user_max_chars,
            channel_max_chars: config.summarization_max_tokens,
            activity_gate_hours: config.autodream_activity_gate_hours,
            activity_min_messages: config.autodream_activity_min_messages,
            channel_activity_hours,
        }
    }
}

pub struct AutoDreamService {
    db: Database,
    cache: MessageCache,
    llm: LlmClient,
    policy: AutoDreamPolicy,
}

impl AutoDreamService {
    pub fn new(db: Database, cache: MessageCache, llm: LlmClient, config: &Config) -> Self {
        Self {
            db,
            cache,
            llm,
            policy: AutoDreamPolicy::from_config(config),
        }
    }

    /// One maintenance cycle: consolidate eligible user profiles and optionally channel working memory.
    pub async fn run_cycle(&self) -> anyhow::Result<()> {
        if self.policy.activity_min_messages > 0 {
            let gate_h = self.policy.activity_gate_hours;
            let min_m = self.policy.activity_min_messages;
            let cache_cnt = self.cache.count_messages_in_window_hours(gate_h);
            let db_cnt = self
                .db
                .run_blocking(move |db| db.count_messages_in_window_hours(gate_h))
                .await?;
            let activity = db_cnt.max(cache_cnt);
            if activity < min_m {
                tracing::debug!(
                    "AutoDream cycle skipped: activity {} in last {}h (db {}, cache {}; min {})",
                    activity,
                    gate_h,
                    db_cnt,
                    cache_cnt,
                    min_m
                );
                return Ok(());
            }
        }

        let cutoff = (Utc::now() - Duration::hours(self.policy.min_hours_between))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let user_ids = self
            .db
            .run_blocking({
                let cutoff = cutoff.clone();
                let lim = self.policy.max_users_per_cycle;
                move |db| db.list_user_ids_due_autodream(&cutoff, lim)
            })
            .await?;

        for user_id in user_ids {
            if let Err(e) = self.consolidate_user(&user_id).await {
                warn!(user_id = %user_id, error = %e, "AutoDream user consolidation failed");
                let _ = self
                    .db
                    .run_blocking({
                        let user_id = user_id.clone();
                        let line = format!(
                            "user {} error: {}",
                            user_id,
                            truncate_err(&e.to_string())
                        );
                        move |db| db.append_autodream_log(&line)
                    })
                    .await;
            }
        }

        if self.policy.channel_summaries {
            let ch_activity = self.policy.channel_activity_hours;
            let channel_ids = self
                .db
                .run_blocking({
                    let cutoff = cutoff.clone();
                    let lim = self.policy.max_channels_per_cycle;
                    move |db| db.list_channel_ids_due_autodream(&cutoff, lim, ch_activity)
                })
                .await?;

            for channel_id in channel_ids {
                if let Err(e) = self.consolidate_channel(&channel_id).await {
                    warn!(channel_id = %channel_id, error = %e, "AutoDream channel consolidation failed");
                    let _ = self
                        .db
                        .run_blocking({
                            let channel_id = channel_id.clone();
                            let line = format!(
                                "channel {} error: {}",
                                channel_id,
                                truncate_err(&e.to_string())
                            );
                            move |db| db.append_autodream_log(&line)
                        })
                        .await;
                }
            }
        }

        Ok(())
    }

    async fn consolidate_user(&self, user_id: &str) -> anyhow::Result<()> {
        let record = self
            .db
            .run_blocking({
                let user_id = user_id.to_string();
                move |db| db.get_user_memory(&user_id)
            })
            .await?;

        let Some(record) = record else {
            return Ok(());
        };
        if !record.enabled || record.summary.trim().is_empty() {
            return Ok(());
        }

        let expected_updated_at = record.updated_at.clone();
        let current = record.summary.trim();
        let prompt = format!(
            "You consolidate a user's global memory profile for a Discord assistant. \
Remove duplicate bullets, resolve contradictions (prefer more recent or more specific facts), \
drop vague relative time phrases that no longer anchor to anything useful, \
and keep only durable preferences and stable facts. Do NOT invent new facts. \
If nothing should change, respond with exactly: NO_UPDATE.\n\n\
Return at most 6 bullet points. Max {} characters total.\n\n\
CURRENT MEMORY:\n{}",
            self.policy.user_max_chars, current
        );

        let raw = self.llm.completion(&prompt).await?;
        let normalized = normalize_memory(&raw, self.policy.user_max_chars);

        if normalized.is_empty() {
            let matched = self
                .db
                .run_blocking({
                    let user_id = user_id.to_string();
                    let expected = expected_updated_at.clone();
                    move |db| db.touch_user_autodream_at_cas(&user_id, &expected)
                })
                .await?;
            if !matched {
                warn!(
                    user_id = %user_id,
                    "AutoDream skipped user memory (concurrent update; will retry later)"
                );
                self.log_line(&format!(
                    "user {}: skipped NO_UPDATE (concurrent writer)",
                    user_id
                ))
                .await?;
                return Ok(());
            }
            self.log_line(&format!("user {}: no change (NO_UPDATE or empty)", user_id))
                .await?;
            return Ok(());
        }

        if normalized == current {
            let matched = self
                .db
                .run_blocking({
                    let user_id = user_id.to_string();
                    let expected = expected_updated_at.clone();
                    move |db| db.touch_user_autodream_at_cas(&user_id, &expected)
                })
                .await?;
            if !matched {
                warn!(
                    user_id = %user_id,
                    "AutoDream skipped user memory (concurrent update; will retry later)"
                );
                self.log_line(&format!(
                    "user {}: skipped unchanged (concurrent writer)",
                    user_id
                ))
                .await?;
                return Ok(());
            }
            self.log_line(&format!("user {}: unchanged after normalize", user_id))
                .await?;
            return Ok(());
        }

        let matched = self
            .db
            .run_blocking({
                let user_id = user_id.to_string();
                let normalized = normalized.clone();
                let exp = record.expires_at.clone();
                let expected = expected_updated_at.clone();
                move |db| db.update_user_memory_autodream_cas(&user_id, &normalized, exp, &expected)
            })
            .await?;
        if !matched {
            warn!(
                user_id = %user_id,
                "AutoDream skipped user memory write (concurrent update; will retry later)"
            );
            self.log_line(&format!(
                "user {}: skipped write (concurrent writer)",
                user_id
            ))
            .await?;
            return Ok(());
        }

        info!(user_id = %user_id, "AutoDream updated user memory");
        self.log_line(&format!(
            "user {}: updated ({} -> {} chars)",
            user_id,
            current.chars().count(),
            normalized.chars().count()
        ))
        .await?;
        Ok(())
    }

    async fn consolidate_channel(&self, channel_id: &str) -> anyhow::Result<()> {
        let record = self
            .db
            .run_blocking({
                let channel_id = channel_id.to_string();
                move |db| db.get_summary_record(&channel_id)
            })
            .await?;

        let Some(record) = record else {
            return Ok(());
        };
        let expected_updated_at = record.updated_at.clone();
        let current = record.summary.trim();
        if current.is_empty() {
            return Ok(());
        }

        let prompt = format!(
            "You consolidate a channel conversation summary for a Discord bot's working memory. \
Remove redundancy, fix contradictions (prefer newer information), remove references to topics that are clearly obsolete, \
and keep a coherent narrative or bullet list. Do NOT invent events that were not implied by the text. \
If nothing should change, respond with exactly: NO_UPDATE.\n\n\
Max approximately {} characters.\n\n\
CURRENT SUMMARY:\n{}",
            self.policy.channel_max_chars, current
        );

        let raw = self.llm.completion(&prompt).await?;
        let normalized = normalize_channel_consolidation(&raw, self.policy.channel_max_chars);

        if normalized.is_empty() {
            let matched = self
                .db
                .run_blocking({
                    let channel_id = channel_id.to_string();
                    let expected = expected_updated_at.clone();
                    move |db| db.touch_channel_autodream_at_cas(&channel_id, &expected)
                })
                .await?;
            if !matched {
                warn!(
                    channel_id = %channel_id,
                    "AutoDream skipped channel (rolling summary wrote first; will retry later)"
                );
                self.log_line(&format!(
                    "channel {}: skipped NO_UPDATE (concurrent rolling summary)",
                    channel_id
                ))
                .await?;
                return Ok(());
            }
            self.log_line(&format!(
                "channel {}: no change (NO_UPDATE or empty)",
                channel_id
            ))
                .await?;
            return Ok(());
        }

        if normalized == current {
            let matched = self
                .db
                .run_blocking({
                    let channel_id = channel_id.to_string();
                    let expected = expected_updated_at.clone();
                    move |db| db.touch_channel_autodream_at_cas(&channel_id, &expected)
                })
                .await?;
            if !matched {
                warn!(
                    channel_id = %channel_id,
                    "AutoDream skipped channel (rolling summary wrote first; will retry later)"
                );
                self.log_line(&format!(
                    "channel {}: skipped unchanged (concurrent rolling summary)",
                    channel_id
                ))
                .await?;
                return Ok(());
            }
            self.log_line(&format!(
                "channel {}: unchanged after normalize",
                channel_id
            ))
                .await?;
            return Ok(());
        }

        let matched = self
            .db
            .run_blocking({
                let channel_id = channel_id.to_string();
                let normalized = normalized.clone();
                let expected = expected_updated_at.clone();
                move |db| {
                    db.update_channel_summary_autodream_cas(&channel_id, &normalized, &expected)
                }
            })
            .await?;
        if !matched {
            warn!(
                channel_id = %channel_id,
                "AutoDream skipped channel write (rolling summary updated summary; will retry later)"
            );
            self.log_line(&format!(
                "channel {}: skipped write (concurrent rolling summary)",
                channel_id
            ))
            .await?;
            return Ok(());
        }

        // Keep milestones aligned with consolidated text (same extractor as rolling summarize).
        let mid = channel_id.to_string();
        let norm = normalized.clone();
        let llm = self.llm.clone();
        let db = self.db.clone();
        if let Ok(milestones) = extract_milestones_for_summary(&llm, &norm).await {
            if !milestones.is_empty() {
                if let Err(e) = db
                    .run_blocking(move |db| db.replace_channel_milestones(&mid, &milestones))
                    .await
                {
                    warn!(
                        channel_id = %channel_id,
                        error = %e,
                        "AutoDream: failed to refresh milestones after consolidation"
                    );
                }
            }
        }

        info!(channel_id = %channel_id, "AutoDream updated channel summary");
        self.log_line(&format!(
            "channel {}: updated ({} -> {} chars)",
            channel_id,
            current.chars().count(),
            normalized.chars().count()
        ))
        .await?;
        Ok(())
    }

    async fn log_line(&self, line: &str) -> anyhow::Result<()> {
        let line = line.to_string();
        self.db
            .run_blocking(move |db| db.append_autodream_log(&line))
            .await
    }
}

fn truncate_err(s: &str) -> String {
    let mut t: String = s.chars().take(200).collect();
    if s.chars().count() > 200 {
        t.push_str("...");
    }
    t
}

fn normalize_channel_consolidation(raw: &str, max_chars: usize) -> String {
    let mut text = raw.trim().replace('\r', "");
    if text.is_empty() {
        return String::new();
    }
    let upper = text.to_uppercase();
    if upper.contains("NO_UPDATE") {
        return String::new();
    }
    for prefix in ["UPDATED SUMMARY:", "SUMMARY:", "CONSOLIDATED:", "OUTPUT:"] {
        if upper.starts_with(prefix) {
            if let Some(stripped) = text.get(prefix.len()..) {
                text = stripped.trim().to_string();
            }
            break;
        }
    }
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
    fn normalize_channel_strips_no_update() {
        assert!(normalize_channel_consolidation("NO_UPDATE", 100).is_empty());
        assert!(normalize_channel_consolidation("  no_update  ", 100).is_empty());
    }
}
