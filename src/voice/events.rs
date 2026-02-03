use serenity::async_trait;
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};
use std::sync::Arc;
use tracing::info;

pub struct IdleHandler {
    pub guild_id: serenity::model::id::GuildId,
    pub manager: Arc<songbird::Songbird>,
    pub idle_timeout_secs: u64,
}

#[async_trait]
impl VoiceEventHandler for IdleHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            // Check if queue is empty after track end
            if track_list.is_empty() {
                let manager = self.manager.clone();
                let guild_id = self.guild_id;

                let idle_timeout = self.idle_timeout_secs;

                // Start a background task to wait and then re-check
                tokio::spawn(async move {
                    info!(
                        "Voice queue empty in guild {}, starting {}-second idle timer...",
                        guild_id, idle_timeout
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(idle_timeout)).await;

                    if let Some(handler_lock) = manager.get(guild_id) {
                        let handler = handler_lock.lock().await;
                        if handler.queue().is_empty() {
                            info!("Idle timer expired in guild {}, leaving channel.", guild_id);
                            drop(handler);
                            let _ = manager.remove(guild_id).await;
                        } else {
                            info!(
                                "Idle timer aborted in guild {}, new tracks found in queue.",
                                guild_id
                            );
                        }
                    }
                });
            }
        }
        None
    }
}
