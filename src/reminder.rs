use crate::db::{Database, DueReminderRecord};
use anyhow::Context as AnyhowContext;
use chrono::{DateTime, Duration, NaiveDateTime, NaiveTime, Utc};
use poise::serenity_prelude::{self as serenity, CreateAllowedMentions, CreateMessage, UserId};
use thiserror::Error;
use tokio::time::{Duration as TokioDuration, MissedTickBehavior};
use tracing::{debug, error, info, warn};

const REMINDER_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const REMINDER_MAX_PENDING_PER_USER: usize = 50;
const REMINDER_BATCH_SIZE: usize = 25;
const REMINDER_MAX_DELIVERY_ATTEMPTS: i64 = 3;
const REMINDER_MIN_LEAD_SECS: i64 = 10;

pub const REMINDER_POLL_INTERVAL_SECS: u64 = 15;
pub const REMINDER_MAX_MINUTES: i64 = 60 * 24 * 30;
pub const REMINDER_MAX_MESSAGE_LEN: usize = 500;

#[derive(Debug, Clone)]
pub struct CreatedReminder {
    pub id: i64,
    pub remind_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PendingReminder {
    pub id: i64,
    pub channel_id: String,
    pub message: String,
    pub remind_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum ReminderServiceError {
    #[error("Reminder must be at least 10 seconds from now.")]
    DelayTooShort,

    #[error("Reminder must be no more than 30 days from now.")]
    DelayTooLong,

    #[error("Reminder message cannot be empty.")]
    EmptyMessage,

    #[error("Reminder message cannot exceed 500 characters.")]
    MessageTooLong,

    #[error(
        "You already have 50 pending reminders in this server. Cancel one before adding more."
    )]
    TooManyPendingReminders,

    #[error("{0}")]
    InvalidSchedule(String),

    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ReminderService {
    db: Database,
}

impl ReminderService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn create_from_schedule_input(
        &self,
        guild_id: &str,
        channel_id: &str,
        user_id: &str,
        schedule_input: &str,
        message: &str,
    ) -> Result<CreatedReminder, ReminderServiceError> {
        let now = Utc::now();
        let remind_at = parse_schedule_input(schedule_input, now)?;
        self.create_at(guild_id, channel_id, user_id, remind_at, message, now)
    }

    fn create_at(
        &self,
        guild_id: &str,
        channel_id: &str,
        user_id: &str,
        remind_at: DateTime<Utc>,
        message: &str,
        now: DateTime<Utc>,
    ) -> Result<CreatedReminder, ReminderServiceError> {
        let min_time = now + Duration::seconds(REMINDER_MIN_LEAD_SECS);
        if remind_at < min_time {
            return Err(ReminderServiceError::DelayTooShort);
        }

        let max_time = now + Duration::minutes(REMINDER_MAX_MINUTES);
        if remind_at > max_time {
            return Err(ReminderServiceError::DelayTooLong);
        }

        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(ReminderServiceError::EmptyMessage);
        }
        if trimmed.chars().count() > REMINDER_MAX_MESSAGE_LEN {
            return Err(ReminderServiceError::MessageTooLong);
        }

        let pending = self
            .db
            .count_pending_reminders_for_user(guild_id, user_id)?;
        if pending >= REMINDER_MAX_PENDING_PER_USER {
            return Err(ReminderServiceError::TooManyPendingReminders);
        }

        let remind_at_str = remind_at.format(REMINDER_TIME_FORMAT).to_string();
        let id = self
            .db
            .create_reminder(guild_id, channel_id, user_id, trimmed, &remind_at_str)?;

        Ok(CreatedReminder { id, remind_at })
    }

    pub fn list_pending_for_user(
        &self,
        guild_id: &str,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<PendingReminder>, ReminderServiceError> {
        let rows = self
            .db
            .list_pending_reminders_for_user(guild_id, user_id, limit)?;

        let mut reminders = Vec::new();
        for row in rows {
            let Some(remind_at) = parse_sqlite_timestamp(&row.remind_at) else {
                warn!(
                    "Skipping reminder {} due to invalid remind_at format: {}",
                    row.id, row.remind_at
                );
                continue;
            };

            reminders.push(PendingReminder {
                id: row.id,
                channel_id: row.channel_id,
                message: row.message,
                remind_at,
            });
        }

        Ok(reminders)
    }

    pub fn cancel_for_user(
        &self,
        guild_id: &str,
        user_id: &str,
        reminder_id: i64,
    ) -> Result<bool, ReminderServiceError> {
        self.db
            .cancel_pending_reminder_for_user(reminder_id, guild_id, user_id)
            .map_err(Into::into)
    }

    pub fn reset_stuck_processing(&self) -> anyhow::Result<usize> {
        self.db.reset_processing_reminders()
    }

    pub async fn process_due_reminders(
        &self,
        http: &std::sync::Arc<serenity::Http>,
    ) -> anyhow::Result<usize> {
        let due = self.db.claim_due_reminders(REMINDER_BATCH_SIZE)?;
        if due.is_empty() {
            return Ok(0);
        }

        debug!("Reminder worker claimed {} due reminder(s)", due.len());
        let mut delivered = 0usize;

        for reminder in due {
            match deliver_reminder(http, &reminder).await {
                Ok(()) => {
                    self.db.mark_reminder_sent(reminder.id)?;
                    delivered += 1;
                }
                Err(err) => {
                    warn!(
                        "Reminder delivery failed for id {} (attempt {}): {}",
                        reminder.id,
                        reminder.delivery_attempts + 1,
                        err
                    );
                    self.db.mark_reminder_delivery_failure(
                        reminder.id,
                        REMINDER_MAX_DELIVERY_ATTEMPTS,
                        &err.to_string(),
                    )?;
                }
            }
        }

        Ok(delivered)
    }
}

pub fn start_dispatcher(db: Database, http: std::sync::Arc<serenity::Http>) {
    tokio::spawn(async move {
        let service = ReminderService::new(db);

        match service.reset_stuck_processing() {
            Ok(reset) if reset > 0 => {
                info!(
                    "Reminder worker reset {} processing reminder(s) back to pending",
                    reset
                );
            }
            Ok(_) => {}
            Err(err) => {
                error!(
                    "Reminder worker failed to reset processing reminders: {}",
                    err
                );
            }
        }

        let mut interval =
            tokio::time::interval(TokioDuration::from_secs(REMINDER_POLL_INTERVAL_SECS));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            match service.process_due_reminders(&http).await {
                Ok(0) => {}
                Ok(count) => {
                    info!("Reminder worker delivered {} reminder(s)", count);
                }
                Err(err) => {
                    error!("Reminder worker cycle failed: {}", err);
                }
            }
        }
    });
}

async fn deliver_reminder(
    http: &std::sync::Arc<serenity::Http>,
    reminder: &DueReminderRecord,
) -> anyhow::Result<()> {
    let channel_id = reminder
        .channel_id
        .parse::<u64>()
        .with_context(|| format!("Invalid channel ID '{}'", reminder.channel_id))?;
    let user_id = reminder
        .user_id
        .parse::<u64>()
        .with_context(|| format!("Invalid user ID '{}'", reminder.user_id))?;

    let remind_at = parse_sqlite_timestamp(&reminder.remind_at).with_context(|| {
        format!(
            "Invalid remind_at '{}' for reminder {}",
            reminder.remind_at, reminder.id
        )
    })?;
    let unix_ts = remind_at.timestamp();

    let content = format!(
        "⏰ <@{}> reminder: {}\nScheduled for <t:{}:F> (<t:{}:R>)",
        reminder.user_id, reminder.message, unix_ts, unix_ts
    );

    let allowed_mentions = CreateAllowedMentions::new()
        .everyone(false)
        .all_roles(false)
        .users(vec![UserId::new(user_id)]);

    serenity::ChannelId::new(channel_id)
        .send_message(
            http,
            CreateMessage::new()
                .content(content)
                .allowed_mentions(allowed_mentions),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to deliver reminder {} to channel {}",
                reminder.id, reminder.channel_id
            )
        })?;

    Ok(())
}

fn parse_schedule_input(
    input: &str,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, ReminderServiceError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(invalid_schedule_error(raw));
    }

    if raw.to_ascii_lowercase().starts_with("at ") {
        return parse_clock_time(raw, now).ok_or_else(|| invalid_schedule_error(raw));
    }

    if let Some(absolute) = parse_absolute_datetime(raw) {
        return Ok(absolute);
    }

    if let Some(relative) = parse_relative_duration(raw, now) {
        return Ok(relative);
    }

    if raw.contains(':') {
        if let Some(clock) = parse_clock_time(raw, now) {
            return Ok(clock);
        }
    }

    Err(invalid_schedule_error(raw))
}

fn parse_relative_duration(input: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let trimmed = input.trim();

    let cleaned = trimmed.replace(',', " ");
    let mut parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("in"))
    {
        parts.remove(0);
    }
    parts.retain(|part| !part.eq_ignore_ascii_case("and"));
    let normalized = collapse_whitespace(&parts.join(" "));

    if normalized.is_empty() {
        return None;
    }

    let parsed = humantime::parse_duration(&normalized).ok()?;
    let duration = Duration::from_std(parsed).ok()?;
    Some(now + duration)
}

fn parse_clock_time(input: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let mut source = input.trim();
    if source.to_ascii_lowercase().starts_with("at ") {
        source = source.get(3..)?.trim();
    }

    if source.is_empty() {
        return None;
    }

    let compact = source.replace(' ', "");
    let mut candidates = vec![
        source.to_string(),
        compact.clone(),
        compact.to_ascii_uppercase(),
    ];
    candidates.sort();
    candidates.dedup();

    let formats = ["%H:%M", "%H:%M:%S", "%I:%M%p", "%I%p"];

    let parsed_time = candidates.iter().find_map(|candidate| {
        formats
            .iter()
            .find_map(|fmt| NaiveTime::parse_from_str(candidate, fmt).ok())
    })?;

    let mut scheduled =
        DateTime::<Utc>::from_naive_utc_and_offset(now.date_naive().and_time(parsed_time), Utc);

    if scheduled <= now {
        scheduled += Duration::days(1);
    }

    Some(scheduled)
}

fn parse_absolute_datetime(input: &str) -> Option<DateTime<Utc>> {
    let raw = input.trim();
    let formats = [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%dT%H:%M:%S",
    ];

    for fmt in formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }

    None
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn invalid_schedule_error(input: &str) -> ReminderServiceError {
    ReminderServiceError::InvalidSchedule(format!(
        "Could not parse reminder time '{}'. Try formats like: 'in 2 days, 30 minutes', '3 hours', 'at 5:30PM', 'at 22:15', or '2026-02-10 17:30' (UTC).",
        input
    ))
}

fn parse_sqlite_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(ts, REMINDER_TIME_FORMAT).ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn fixed_now() -> DateTime<Utc> {
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-10 12:00:00", REMINDER_TIME_FORMAT).unwrap(),
            Utc,
        )
    }

    fn test_config() -> Config {
        Config {
            discord_token: "test".to_string(),
            application_id: 0,
            owner_id: Some(1),
            llama_url: "test".to_string(),
            llama_model: "test".to_string(),
            llama_api_key: None,
            embedding_url: "test".to_string(),
            embedding_model: "test".to_string(),
            embedding_api_key: None,
            database_url: ":memory:".to_string(),
            system_prompt: "test".to_string(),
            max_context_messages: 10,
            status_message: "test".to_string(),
            youtube_cookies: None,
            youtube_download_dir: "/tmp".to_string(),
            youtube_cleanup_after_secs: 3600,
            mcp_servers: Vec::new(),
            context_message_limit: 50,
            context_retention_hours: 24,
            llm_timeout_secs: 120,
            embedding_timeout_secs: 30,
            mcp_timeout_secs: 60,
            voice_idle_timeout_secs: 300,
            dev_guild_id: None,
            register_commands: false,
            mcp_tools_require_confirmation: true,
            agent_confirm_timeout_secs: 300,
            embedding_indexer_enabled: true,
            embedding_indexer_batch_size: 25,
            embedding_indexer_interval_secs: 30,
            summarization_enabled: true,
            summarization_interval_secs: 3600,
            summarization_active_channels_lookback_days: 7,
            summarization_initial_min_messages: 50,
            summarization_trigger_new_messages: 150,
            summarization_trigger_age_hours: 6,
            summarization_trigger_min_new_messages: 20,
            summarization_max_tokens: 1200,
            summarization_refresh_weeks: 6,
            summarization_refresh_days_lookback: 14,
            long_term_retention_days: 365,
        }
    }

    #[test]
    fn parse_relative_duration_with_in_and_comma() {
        let now = fixed_now();
        let parsed = parse_schedule_input("in 2 days, 30 minutes", now).unwrap();
        assert_eq!(parsed, now + Duration::days(2) + Duration::minutes(30));
    }

    #[test]
    fn parse_relative_duration_without_prefix() {
        let now = fixed_now();
        let parsed = parse_schedule_input("3 hours", now).unwrap();
        assert_eq!(parsed, now + Duration::hours(3));
    }

    #[test]
    fn parse_relative_duration_case_insensitive_prefixes() {
        let now = fixed_now();
        let parsed = parse_schedule_input("In 3 hours and 15 minutes", now).unwrap();
        assert_eq!(parsed, now + Duration::hours(3) + Duration::minutes(15));
    }

    #[test]
    fn parse_plain_number_is_invalid_without_unit() {
        let now = fixed_now();
        let err = parse_schedule_input("45", now).unwrap_err();
        match err {
            ReminderServiceError::InvalidSchedule(_) => {}
            _ => panic!("Expected InvalidSchedule"),
        }
    }

    #[test]
    fn parse_clock_time_24h() {
        let now = fixed_now();
        let parsed = parse_schedule_input("at 22:15", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-10 22:15:00", REMINDER_TIME_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_clock_time_12h() {
        let now = fixed_now();
        let parsed = parse_schedule_input("at 5:30PM", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-10 17:30:00", REMINDER_TIME_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_clock_time_rolls_to_next_day_if_past() {
        let now = fixed_now();
        let parsed = parse_schedule_input("at 11:30", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-11 11:30:00", REMINDER_TIME_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_clock_time_without_at_prefix() {
        let now = fixed_now();
        let parsed = parse_schedule_input("22:15", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-10 22:15:00", REMINDER_TIME_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_absolute_datetime() {
        let now = fixed_now();
        let parsed = parse_schedule_input("2026-02-12 08:45", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-12 08:45:00", REMINDER_TIME_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_invalid_schedule_returns_error() {
        let err = parse_schedule_input("sometime later maybe", fixed_now()).unwrap_err();
        match err {
            ReminderServiceError::InvalidSchedule(_) => {}
            _ => panic!("Expected invalid schedule error"),
        }
    }

    #[test]
    fn create_from_schedule_rejects_too_short() {
        let db = Database::new(&test_config()).unwrap();
        db.execute_init().unwrap();
        let service = ReminderService::new(db);

        let err = service
            .create_from_schedule_input("g1", "c1", "u1", "in 5 seconds", "too soon")
            .unwrap_err();

        match err {
            ReminderServiceError::DelayTooShort => {}
            _ => panic!("Expected DelayTooShort"),
        }
    }

    #[test]
    fn create_from_schedule_rejects_too_long() {
        let db = Database::new(&test_config()).unwrap();
        db.execute_init().unwrap();
        let service = ReminderService::new(db);

        let err = service
            .create_from_schedule_input("g1", "c1", "u1", "in 31 days", "too far")
            .unwrap_err();

        match err {
            ReminderServiceError::DelayTooLong => {}
            _ => panic!("Expected DelayTooLong"),
        }
    }

    #[test]
    fn create_from_schedule_rejects_month_precision_overflow() {
        let db = Database::new(&test_config()).unwrap();
        db.execute_init().unwrap();
        let service = ReminderService::new(db);

        // humantime treats 'M' as month, which is slightly >30 days.
        let err = service
            .create_from_schedule_input("g1", "c1", "u1", "in 1M", "too far")
            .unwrap_err();

        match err {
            ReminderServiceError::DelayTooLong => {}
            _ => panic!("Expected DelayTooLong"),
        }
    }

    #[test]
    fn create_from_schedule_persists_reminder() {
        let db = Database::new(&test_config()).unwrap();
        db.execute_init().unwrap();
        let service = ReminderService::new(db.clone());

        let created = service
            .create_from_schedule_input("g1", "c1", "u1", "in 3 hours", "ship build")
            .unwrap();
        assert!(created.id > 0);

        let pending = service.list_pending_for_user("g1", "u1", 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].message, "ship build");
    }
}
