use crate::config::Config;
use crate::db::ChannelSummaryRecord;
use crate::db::Database;
use crate::llm::LlmClient;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use tracing::{info, warn};

#[derive(Clone)]
pub struct SummarizationPolicy {
    pub active_channels_lookback_days: i64,
    pub initial_min_messages: usize,
    pub trigger_new_messages: usize,
    pub trigger_age_hours: i64,
    pub trigger_min_new_messages: usize,
    pub max_tokens: usize,
    pub refresh_weeks: i64,
    pub refresh_days_lookback: i64,
}

impl SummarizationPolicy {
    fn from_config(config: &Config) -> Self {
        Self {
            active_channels_lookback_days: config.summarization_active_channels_lookback_days,
            initial_min_messages: config.summarization_initial_min_messages,
            trigger_new_messages: config.summarization_trigger_new_messages,
            trigger_age_hours: config.summarization_trigger_age_hours,
            trigger_min_new_messages: config.summarization_trigger_min_new_messages,
            max_tokens: config.summarization_max_tokens,
            refresh_weeks: config.summarization_refresh_weeks,
            refresh_days_lookback: config.summarization_refresh_days_lookback,
        }
    }
}

pub struct SummarizationManager {
    db: Database,
    llm: LlmClient,
    policy: SummarizationPolicy,
}

impl SummarizationManager {
    pub fn new(db: Database, llm: LlmClient, config: &Config) -> Self {
        Self {
            db,
            llm,
            policy: SummarizationPolicy::from_config(config),
        }
    }

    /// Summarizes the last N messages for a channel and saves the summary
    pub async fn summarize_channel(&self, channel_id: &str, days: i64) -> anyhow::Result<()> {
        info!("Starting summarization for channel: {}", channel_id);

        let now = Utc::now();
        let channel_id_str = channel_id.to_string();
        let record = self
            .db
            .run_blocking(move |db| db.get_summary_record(&channel_id_str))
            .await?;

        let refresh_due = record
            .as_ref()
            .and_then(|r| parse_sqlite_utc(&r.refreshed_at))
            .is_some_and(|ts| now - ts > Duration::weeks(self.policy.refresh_weeks));

        // For normal updates: only summarize new messages since last `updated_at`.
        // For first summaries: use the last `days` days.
        // For periodic refresh: rebuild from the last 14 days.
        let from_ts = if refresh_due {
            now - Duration::days(self.policy.refresh_days_lookback)
        } else if let Some(r) = record
            .as_ref()
            .and_then(|r| parse_sqlite_utc(&r.updated_at))
        {
            r
        } else {
            now - Duration::days(days)
        };

        let channel_id_str = channel_id.to_string();
        let messages = self
            .db
            .run_blocking(move |db| db.get_recent_messages(&channel_id_str, from_ts, 200))
            .await?;

        let channel_id_str = channel_id.to_string();
        let milestones = self
            .db
            .run_blocking(move |db| db.get_channel_milestones(&channel_id_str, 20))
            .await
            .unwrap_or_default();

        if messages.is_empty() {
            info!("No messages to summarize for channel: {}", channel_id);
            return Ok(());
        }

        // 2. Format messages for the summarizer
        let mut text_to_summarize = String::new();
        for msg in messages.iter().rev() {
            // chronological
            text_to_summarize.push_str(&format!(
                "[{}] {}: {}\n",
                msg.timestamp, msg.user_id, msg.content
            ));
        }

        // 3. Prompt the LLM (rolling summary pattern)
        let prompt = build_summary_prompt(
            record.as_ref(),
            refresh_due,
            &milestones,
            &text_to_summarize,
        );

        let summary = self.llm.completion(&prompt).await?;
        let summary = self
            .enforce_summary_cap(&summary, self.policy.max_tokens)
            .await?;

        // 4. Save to DB
        if refresh_due {
            let channel_id_str = channel_id.to_string();
            let summary_clone = summary.clone();
            self.db
                .run_blocking(move |db| db.save_summary_refresh(&channel_id_str, &summary_clone))
                .await?;
        } else {
            let channel_id_str = channel_id.to_string();
            let summary_clone = summary.clone();
            self.db
                .run_blocking(move |db| db.save_summary(&channel_id_str, &summary_clone))
                .await?;
        }

        if let Ok(milestones) = self.extract_milestones(&summary).await {
            if !milestones.is_empty() {
                let channel_id_str = channel_id.to_string();
                if let Err(e) = self
                    .db
                    .run_blocking(move |db| {
                        db.replace_channel_milestones(&channel_id_str, &milestones)
                    })
                    .await
                {
                    warn!(
                        "Failed to persist milestones for channel {}: {}",
                        channel_id, e
                    );
                }
            }
        }

        info!("Successfully summarized channel: {}", channel_id);
        Ok(())
    }

    pub async fn get_active_channels(&self) -> anyhow::Result<Vec<String>> {
        let db = self.db.clone();
        let lookback_days = self.policy.active_channels_lookback_days;
        tokio::task::spawn_blocking(move || db.get_channels_with_activity(lookback_days)).await?
    }

    pub async fn should_summarize_channel(&self, channel_id: &str) -> anyhow::Result<bool> {
        let now = Utc::now();
        let channel_id_str = channel_id.to_string();
        let record = self
            .db
            .run_blocking(move |db| db.get_summary_record(&channel_id_str))
            .await?;

        let refresh_due = record
            .as_ref()
            .and_then(|r| parse_sqlite_utc(&r.refreshed_at))
            .is_some_and(|ts| now - ts > Duration::weeks(self.policy.refresh_weeks));

        let updated_at = record.as_ref().map(|r| r.updated_at.as_str()).unwrap_or("");

        let new_messages = if updated_at.is_empty() {
            let since = (now - Duration::hours(24))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            let channel_id_str = channel_id.to_string();
            self.db
                .run_blocking(move |db| db.count_channel_messages_since(&channel_id_str, &since))
                .await?
        } else {
            let channel_id_str = channel_id.to_string();
            let updated_at = updated_at.to_string();
            self.db
                .run_blocking(move |db| {
                    db.count_channel_messages_since(&channel_id_str, &updated_at)
                })
                .await?
        };

        // Initial summaries: require a little more activity to avoid spammy summaries.
        if record.is_none() {
            return Ok(new_messages >= self.policy.initial_min_messages);
        }

        let summary_age_hours = record
            .as_ref()
            .and_then(|r| parse_sqlite_utc(&r.updated_at))
            .map(|ts| (now - ts).num_hours())
            .unwrap_or(999);

        if refresh_due && new_messages > 0 {
            return Ok(true);
        }

        Ok(new_messages >= self.policy.trigger_new_messages
            || (summary_age_hours >= self.policy.trigger_age_hours
                && new_messages >= self.policy.trigger_min_new_messages))
    }

    async fn enforce_summary_cap(
        &self,
        summary: &str,
        max_tokens: usize,
    ) -> anyhow::Result<String> {
        // If we don't have an exact tokenizer, approximate: ~4 chars per token for English.
        let approx_tokens = summary.chars().count() / 4;
        if approx_tokens <= max_tokens {
            return Ok(summary.to_string());
        }

        warn!(
            "Summary exceeds cap (approx {} tokens > {}); compressing",
            approx_tokens, max_tokens
        );

        // Try up to two compression passes.
        let mut current = summary.to_string();
        for _ in 0..2 {
            let prompt = format!(
                "Condense the following channel summary to be under {max_tokens} tokens. \
Keep it accurate and preserve key decisions, constraints, and ongoing threads.\n\nSUMMARY:\n{current}\n\nCONDENSED SUMMARY:"
            );
            current = self.llm.completion(&prompt).await?;
            let approx = current.chars().count() / 4;
            if approx <= max_tokens {
                break;
            }
        }

        Ok(current)
    }

    async fn extract_milestones(&self, summary: &str) -> anyhow::Result<Vec<String>> {
        const MAX_MILESTONES: usize = 6;
        let prompt = format!(
            "Extract up to {MAX_MILESTONES} durable milestones (decisions, commitments, constraints, or ongoing threads) \
from the summary below. Respond with one per line prefixed with '- '. If there are none, respond with 'None'.\n\n\
SUMMARY:\n{summary}\n\nMILESTONES:"
        );

        let raw = self.llm.completion(&prompt).await?;
        Ok(parse_milestones(&raw, MAX_MILESTONES))
    }
}

fn parse_sqlite_utc(ts: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn build_summary_prompt(
    record: Option<&ChannelSummaryRecord>,
    refresh_due: bool,
    milestones: &[String],
    new_messages: &str,
) -> String {
    let milestones_block = if milestones.is_empty() {
        "(none)".to_string()
    } else {
        milestones.join("\n")
    };

    match (record, refresh_due) {
        (Some(r), true) => format!(
            "Rewrite the channel summary from scratch to reduce drift and improve stability.\n\
Use the previous summary as historical context, and the recent messages as ground truth.\n\
Keep it concise and factual; omit trivial chatter.\n\n\
MILESTONES:\n{milestones_block}\n\n\
PREVIOUS SUMMARY:\n{prev}\n\n\
RECENT MESSAGES:\n{new_messages}\n\n\
REFRESHED SUMMARY:",
            prev = r.summary,
        ),
        (Some(r), false) => format!(
            "You maintain a rolling channel summary. Update the summary using the new messages.\n\
Keep continuity, only add important new information, and remove outdated details.\n\
Prefer durable facts, decisions, and ongoing threads.\n\n\
MILESTONES:\n{milestones_block}\n\n\
PREVIOUS SUMMARY:\n{prev}\n\n\
NEW MESSAGES:\n{new_messages}\n\n\
UPDATED SUMMARY:",
            prev = r.summary,
        ),
        (None, _) => format!(
            "Summarize the following channel messages.\n\
Focus on key topics, decisions, constraints, and ongoing threads; omit trivial chatter.\n\n\
MILESTONES:\n{milestones_block}\n\n\
MESSAGES:\n{new_messages}\n\n\
SUMMARY:"
        ),
    }
}

fn parse_milestones(raw: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("none") {
            if out.is_empty() {
                return Vec::new();
            }
            break;
        }

        let item = if let Some(stripped) = line.strip_prefix("- ") {
            Some(stripped.trim().to_string())
        } else if let Some(stripped) = line.strip_prefix("* ") {
            Some(stripped.trim().to_string())
        } else {
            strip_numbered_prefix(line)
        };

        let Some(item) = item else {
            continue;
        };

        if item.is_empty() {
            continue;
        }

        if seen.insert(item.to_lowercase()) {
            out.push(item);
            if out.len() >= max {
                break;
            }
        }
    }

    out
}

fn strip_numbered_prefix(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let mut end_digits = 0;

    for (idx, c) in trimmed.char_indices() {
        if c.is_ascii_digit() {
            end_digits = idx + c.len_utf8();
        } else {
            break;
        }
    }

    if end_digits == 0 {
        return None;
    }

    let rest = trimmed[end_digits..].trim_start();
    let rest = if let Some(stripped) = rest.strip_prefix('.') {
        stripped.trim_start()
    } else if let Some(stripped) = rest.strip_prefix(')') {
        stripped.trim_start()
    } else {
        return None;
    };

    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_milestones;

    #[test]
    fn test_parse_milestones_handles_bullets_and_numbers() {
        let raw = "MILESTONES:\n- Decision A\n1. Constraint B\n* Ongoing thread\nNone";
        let milestones = parse_milestones(raw, 5);
        assert_eq!(milestones.len(), 3);
        assert_eq!(milestones[0], "Decision A");
        assert_eq!(milestones[1], "Constraint B");
        assert_eq!(milestones[2], "Ongoing thread");
    }
}
