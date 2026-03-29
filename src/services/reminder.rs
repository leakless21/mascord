use crate::db::{Database, ReminderRecord};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, NaiveTime, Utc};
use thiserror::Error;

const SQLITE_UTC_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
pub const REMINDER_MIN_LEAD_SECS: i64 = 60;
pub const REMINDER_MAX_LEAD_DAYS: i64 = 30;
/// Shared with `/reminder` slash command and LLM tools.
pub const MAX_REMINDER_MESSAGE_CHARS: usize = 1500;
pub const MAX_LIST_RESULTS: usize = 20;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReminderScheduleError {
    #[error(
        "Could not parse reminder time. Try `in 2 days, 30 minutes`, `3 hours`, `at 5:30PM`, `at 22:15`, or `2026-02-10 17:30` (UTC)."
    )]
    InvalidSchedule,
    #[error("Reminders must be at least 1 minute in the future.")]
    TooSoon,
    #[error("Reminders must be within 30 days.")]
    TooFar,
}

pub struct ReminderService {
    db: Database,
}

impl ReminderService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn create_reminder(
        &self,
        guild_id: u64,
        channel_id: u64,
        user_id: u64,
        message: &str,
        remind_at: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        let guild_id = guild_id.to_string();
        let channel_id = channel_id.to_string();
        let user_id = user_id.to_string();
        let message = message.to_string();
        let remind_at = remind_at.format("%Y-%m-%d %H:%M:%S").to_string();
        self.db
            .run_blocking(move |db| {
                db.create_reminder(&guild_id, &channel_id, &user_id, &message, &remind_at)
            })
            .await
    }

    pub async fn list_pending_reminders(
        &self,
        user_id: u64,
        limit: usize,
    ) -> anyhow::Result<Vec<ReminderRecord>> {
        let user_id = user_id.to_string();
        self.db
            .run_blocking(move |db| db.list_pending_reminders_for_user(&user_id, limit))
            .await
    }

    pub async fn delete_pending_reminder(
        &self,
        reminder_id: i64,
        user_id: u64,
    ) -> anyhow::Result<usize> {
        let user_id = user_id.to_string();
        self.db
            .run_blocking(move |db| db.delete_pending_reminder(reminder_id, &user_id))
            .await
    }

    pub async fn get_due_reminders(&self, limit: usize) -> anyhow::Result<Vec<ReminderRecord>> {
        self.db
            .run_blocking(move |db| db.get_due_reminders(limit))
            .await
    }

    pub async fn mark_delivered(&self, reminder_id: i64) -> anyhow::Result<()> {
        self.db
            .run_blocking(move |db| db.mark_reminder_delivered(reminder_id))
            .await
    }

    pub fn parse_schedule_input(
        input: &str,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, ReminderScheduleError> {
        let raw = input.trim();
        if raw.is_empty() {
            return Err(ReminderScheduleError::InvalidSchedule);
        }

        let parsed = if raw.to_ascii_lowercase().starts_with("at ") {
            parse_clock_time(raw, now)
        } else {
            parse_absolute_datetime(raw)
                .or_else(|| parse_relative_duration(raw, now))
                .or_else(|| {
                    raw.contains(':')
                        .then(|| parse_clock_time(raw, now))
                        .flatten()
                })
        };

        let remind_at = parsed.ok_or(ReminderScheduleError::InvalidSchedule)?;
        let min_time = now + ChronoDuration::seconds(REMINDER_MIN_LEAD_SECS);
        if remind_at < min_time {
            return Err(ReminderScheduleError::TooSoon);
        }

        let max_time = now + ChronoDuration::days(REMINDER_MAX_LEAD_DAYS);
        if remind_at > max_time {
            return Err(ReminderScheduleError::TooFar);
        }

        Ok(remind_at)
    }

    pub fn parse_sqlite_utc(ts: &str) -> Option<DateTime<Utc>> {
        let naive = NaiveDateTime::parse_from_str(ts, SQLITE_UTC_FORMAT).ok()?;
        Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    }
}

fn parse_relative_duration(input: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let cleaned = input.replace(',', " ");
    let mut parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("in"))
    {
        parts.remove(0);
    }
    parts.retain(|part| !part.eq_ignore_ascii_case("and"));
    let normalized = parts.join(" ");
    if normalized.is_empty() {
        return None;
    }

    let std_duration = humantime::parse_duration(&normalized).ok()?;
    let duration = ChronoDuration::from_std(std_duration).ok()?;
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
    candidates.sort_unstable();
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
        scheduled += ChronoDuration::days(1);
    }
    Some(scheduled)
}

fn parse_absolute_datetime(input: &str) -> Option<DateTime<Utc>> {
    let formats = [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%dT%H:%M:%S",
    ];

    for fmt in formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(input, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now() -> DateTime<Utc> {
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-10 12:00:00", SQLITE_UTC_FORMAT).unwrap(),
            Utc,
        )
    }

    #[test]
    fn parse_relative_duration_with_in_and_comma() {
        let now = fixed_now();
        let parsed = ReminderService::parse_schedule_input("in 2 days, 30 minutes", now).unwrap();
        assert_eq!(
            parsed,
            now + ChronoDuration::days(2) + ChronoDuration::minutes(30)
        );
    }

    #[test]
    fn parse_relative_duration_without_prefix() {
        let now = fixed_now();
        let parsed = ReminderService::parse_schedule_input("3 hours", now).unwrap();
        assert_eq!(parsed, now + ChronoDuration::hours(3));
    }

    #[test]
    fn parse_relative_duration_case_insensitive_prefixes() {
        let now = fixed_now();
        let parsed =
            ReminderService::parse_schedule_input("In 3 hours and 15 minutes", now).unwrap();
        assert_eq!(
            parsed,
            now + ChronoDuration::hours(3) + ChronoDuration::minutes(15)
        );
    }

    #[test]
    fn parse_plain_number_is_invalid_without_unit() {
        let err = ReminderService::parse_schedule_input("45", fixed_now()).unwrap_err();
        assert_eq!(err, ReminderScheduleError::InvalidSchedule);
    }

    #[test]
    fn parse_clock_time_24h() {
        let now = fixed_now();
        let parsed = ReminderService::parse_schedule_input("at 22:15", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-10 22:15:00", SQLITE_UTC_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_clock_time_12h() {
        let now = fixed_now();
        let parsed = ReminderService::parse_schedule_input("at 5:30PM", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-10 17:30:00", SQLITE_UTC_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_clock_time_rolls_to_next_day_if_past() {
        let now = fixed_now();
        let parsed = ReminderService::parse_schedule_input("at 11:30", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-11 11:30:00", SQLITE_UTC_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_absolute_datetime() {
        let now = fixed_now();
        let parsed = ReminderService::parse_schedule_input("2026-02-12 08:45", now).unwrap();
        let expected = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2026-02-12 08:45:00", SQLITE_UTC_FORMAT).unwrap(),
            Utc,
        );
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_too_soon_rejected() {
        let err = ReminderService::parse_schedule_input("in 30 seconds", fixed_now()).unwrap_err();
        assert_eq!(err, ReminderScheduleError::TooSoon);
    }

    #[test]
    fn parse_too_far_rejected() {
        let err = ReminderService::parse_schedule_input("in 31 days", fixed_now()).unwrap_err();
        assert_eq!(err, ReminderScheduleError::TooFar);
    }
}
