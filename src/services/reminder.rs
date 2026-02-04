use crate::db::{Database, ReminderRecord};
use chrono::{DateTime, NaiveDateTime, Utc};

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

    pub fn parse_sqlite_utc(ts: &str) -> Option<DateTime<Utc>> {
        let naive = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()?;
        Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    }
}
