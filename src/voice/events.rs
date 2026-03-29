use crate::commands::music::MusicState;
use serenity::async_trait;
use songbird::tracks::PlayMode;
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};
use std::sync::Arc;
use tracing::{info, warn};

pub struct IdleHandler {
    pub guild_id: serenity::model::id::GuildId,
    pub manager: Arc<songbird::Songbird>,
    pub idle_timeout_secs: u64,
    pub music: Arc<MusicState>,
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
                let music = self.music.clone();
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
                            music.clear_voice_hooks(guild_id.get());
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

/// Skip to the next track when the current track hits a decode/source error (skip-on-error).
pub struct TrackErrorHandler {
    pub guild_id: serenity::model::id::GuildId,
    pub manager: Arc<songbird::Songbird>,
}

#[async_trait]
impl VoiceEventHandler for TrackErrorHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            let mut saw_error = false;
            for (state, _) in track_list.iter() {
                if let PlayMode::Errored(e) = &state.playing {
                    warn!("Track playback error in guild {}: {}", self.guild_id, e);
                    saw_error = true;
                }
            }
            if !saw_error {
                return None;
            }
            if let Some(h) = self.manager.get(self.guild_id) {
                let handler = h.lock().await;
                let queue = handler.queue();
                if !queue.is_empty() {
                    match queue.skip() {
                        Ok(_) => info!(
                            "Advanced queue after playback error in guild {}",
                            self.guild_id
                        ),
                        Err(e) => warn!(
                            "Queue skip after playback error failed in guild {}: {}",
                            self.guild_id, e
                        ),
                    }
                }
            }
        }
        None
    }
}

/// Log voice driver lifecycle and clear pending alone-leave timers on disconnect.
pub struct DriverLifecycleHandler {
    pub guild_id: serenity::model::id::GuildId,
    pub music: Arc<MusicState>,
}

#[async_trait]
impl VoiceEventHandler for DriverLifecycleHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        match ctx {
            EventContext::DriverDisconnect(d) => {
                warn!(
                    "Voice driver disconnected in guild {} (kind: {:?}, reason: {:?})",
                    self.guild_id, d.kind, d.reason
                );
                self.music.cancel_alone_leave_task(self.guild_id.get());
            }
            EventContext::DriverReconnect(c) => {
                info!(
                    "Voice driver reconnected in guild {} (channel {})",
                    self.guild_id, c.channel_id
                );
            }
            EventContext::DriverConnect(c) => {
                tracing::debug!(
                    "Voice driver connected in guild {} (channel {})",
                    self.guild_id,
                    c.channel_id
                );
            }
            _ => {}
        }
        None
    }
}
