use crate::db::ReminderRecord;
use crate::services::reminder::ReminderService;
use anyhow::Context as AnyhowContext;
use chrono::Utc;
use serenity::all::{ChannelId, CreateAllowedMentions, CreateMessage, UserId};
use serenity::http::Http;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error};

pub struct ReminderDispatcher {
    service: ReminderService,
    http: Arc<Http>,
    poll_interval: Duration,
    batch_size: usize,
}

impl ReminderDispatcher {
    pub fn new(
        service: ReminderService,
        http: Arc<Http>,
        poll_interval_secs: u64,
        batch_size: usize,
    ) -> Self {
        Self {
            service,
            http,
            poll_interval: Duration::from_secs(poll_interval_secs),
            batch_size,
        }
    }

    pub async fn run(self) {
        let mut ticker = interval(self.poll_interval);
        loop {
            ticker.tick().await;
            if let Err(e) = self.dispatch_due().await {
                error!("Reminder dispatch cycle failed: {}", e);
            }
        }
    }

    async fn dispatch_due(&self) -> anyhow::Result<()> {
        let reminders = self.service.get_due_reminders(self.batch_size).await?;
        if reminders.is_empty() {
            return Ok(());
        }

        for reminder in reminders {
            match self.send_reminder(&reminder).await {
                Ok(()) => {
                    if let Err(e) = self.service.mark_delivered(reminder.id).await {
                        error!("Failed to mark reminder {} delivered: {}", reminder.id, e);
                    }
                }
                Err(e) => {
                    error!("Failed to send reminder {}: {}", reminder.id, e);
                }
            }
        }

        Ok(())
    }

    async fn send_reminder(&self, reminder: &ReminderRecord) -> anyhow::Result<()> {
        let user_id: u64 = reminder
            .user_id
            .parse()
            .with_context(|| format!("Invalid reminder user_id '{}'", reminder.user_id))?;
        let channel_id: u64 = reminder
            .channel_id
            .parse()
            .with_context(|| format!("Invalid reminder channel_id '{}'", reminder.channel_id))?;

        let remind_at =
            ReminderService::parse_sqlite_utc(&reminder.remind_at).unwrap_or_else(Utc::now);
        let ts = remind_at.timestamp();

        let content = format!(
            "⏰ <@{user_id}> Reminder: {}\nDue: <t:{ts}:F> (<t:{ts}:R>)",
            reminder.message
        );

        let allowed_mentions = CreateAllowedMentions::new().users(vec![UserId::new(user_id)]);
        let builder = CreateMessage::new()
            .content(content)
            .allowed_mentions(allowed_mentions);

        debug!(
            "Dispatching reminder {} to channel {} for user {}",
            reminder.id, channel_id, user_id
        );

        ChannelId::new(channel_id)
            .send_message(&self.http, builder)
            .await?;

        Ok(())
    }
}
