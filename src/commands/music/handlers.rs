use crate::commands::music::playback::replay_queue_snapshot;
use crate::commands::music::state::MusicState;
use serenity::async_trait;
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};
use std::sync::Arc;
use tracing::{info, warn};

/// When the queue drains, optionally re-enqueue the saved queue-loop snapshot.
pub struct QueueLoopHandler {
    pub guild_id: serenity::model::id::GuildId,
    pub manager: Arc<songbird::Songbird>,
    pub music: Arc<MusicState>,
    pub http_client: reqwest::Client,
    pub youtube_cookies: Option<String>,
}

#[async_trait]
impl VoiceEventHandler for QueueLoopHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let EventContext::Track(_) = ctx else {
            return None;
        };

        let guild_id = self.guild_id;
        let manager = self.manager.clone();
        let music = self.music.clone();
        let http = self.http_client.clone();
        let cookies = self.youtube_cookies.clone();

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if !music.queue_loop_enabled(guild_id.get()) {
                return;
            }
            let Some(handler_lock) = manager.get(guild_id) else {
                return;
            };
            {
                let handler = handler_lock.lock().await;
                if !handler.queue().is_empty() {
                    return;
                }
            }
            info!(
                "Queue loop: re-adding {} tracks in guild {}",
                music.queue_loop_snapshot(guild_id.get()).len(),
                guild_id
            );
            if let Err(e) = replay_queue_snapshot(&manager, guild_id, &music, http, cookies).await {
                warn!("Queue loop replay failed: {}", e);
            }
        });

        None
    }
}
