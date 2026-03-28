use crate::commands::music::playback::{enqueue_playback, EnqueueOpts};
use crate::llm::confirm::DiscordToolContext;
use crate::tools::Tool;
use anyhow::Context as _;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

pub struct PlayMusicTool;

#[async_trait]
impl Tool for PlayMusicTool {
    fn name(&self) -> &str {
        "play_music"
    }
    fn description(&self) -> &str {
        "Queue music from a YouTube search, video URL, or other yt-dlp-supported URL. The user must be in a voice channel in a server."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Song title / search query or media URL"
                }
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, _params: Value) -> anyhow::Result<Value> {
        Ok(json!({
            "status": "error",
            "message": "play_music requires a Discord context; use /chat in a server with the bot."
        }))
    }

    async fn execute_with_discord(
        &self,
        params: Value,
        dctx: Option<&DiscordToolContext<'_>>,
    ) -> anyhow::Result<Value> {
        let Some(ctx) = dctx else {
            warn!("play_music: execute_with_discord called without Discord context (agent must use run_with_confirmation)");
            return self.execute(params).await;
        };
        let Some(guild_id) = ctx.guild_id else {
            return Ok(json!({
                "status": "error",
                "message": "Music only works in a server (guild), not in DMs."
            }));
        };
        let query = params
            .get("query")
            .and_then(|q| q.as_str())
            .context("Missing or invalid `query`")?
            .trim()
            .to_string();
        if query.is_empty() {
            return Ok(json!({"status": "error", "message": "Query was empty."}));
        }
        info!(
            guild = guild_id.get(),
            user = ctx.user_id.get(),
            %query,
            "play_music: enqueue_playback"
        );
        match enqueue_playback(
            ctx.serenity_ctx,
            ctx.data,
            guild_id,
            ctx.user_id,
            query.clone(),
            &ctx.data.music,
            EnqueueOpts {
                expand_playlist: false,
                skip_voice_check: false,
            },
        )
        .await
        {
            Ok(s) => {
                info!(
                    guild = guild_id.get(),
                    added = s.added,
                    title = ?s.first_title,
                    "play_music: queued OK"
                );
                Ok(json!({
                    "status": "ok",
                    "added": s.added,
                    "title": s.first_title,
                    "query": query
                }))
            }
            Err(e) => {
                warn!(guild = guild_id.get(), %query, error = %e, "play_music: enqueue failed");
                Ok(json!({
                    "status": "error",
                    "message": e
                }))
            }
        }
    }
}
