//! Single LLM tool `music` — same capabilities as slash music commands via `action`.

use crate::llm::confirm::DiscordToolContext;
use crate::services::music_ops::{
    music_clear, music_join, music_leave, music_loop, music_lyrics, music_move_track,
    music_now_playing, music_pause, music_play, music_queue_tool_value, music_remove, music_resume,
    music_shuffle, music_skip, music_volume, LoopModeArg, MUSIC_AGENT_TOOL_NAME, MUSIC_HELP_TEXT,
};
use crate::tools::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

pub struct MusicTool;

fn parse_action(params: &Value) -> Option<String> {
    params
        .get("action")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
}

/// Core routing for the `music` tool; used by [`MusicTool`] and tests.
pub async fn dispatch_music_tool(
    params: Value,
    serenity_ctx: &poise::serenity_prelude::Context,
    data: &crate::Data,
    guild_id: Option<poise::serenity_prelude::GuildId>,
    user_id: poise::serenity_prelude::UserId,
) -> anyhow::Result<Value> {
    let action = match parse_action(&params) {
        Some(a) => a,
        None => {
            return Ok(json!({
                "status": "error",
                "message": "Missing or empty `action`. Use help for a list."
            }));
        }
    };

    let Some(guild_id) = guild_id else {
        return Ok(json!({
            "status": "error",
            "message": "Music only works in a server (guild), not in DMs."
        }));
    };

    match action.as_str() {
        "help" => Ok(json!({
            "status": "ok",
            "help": MUSIC_HELP_TEXT
        })),

        "join" => match music_join(serenity_ctx, data, guild_id, user_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "play" => {
            let query = params
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if query.is_empty() {
                return Ok(json!({
                    "status": "error",
                    "message": "action=play requires non-empty `query`."
                }));
            }
            let playlist = params
                .get("playlist")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match music_play(serenity_ctx, data, guild_id, user_id, query, playlist).await {
                Ok(op) => Ok(op.to_tool_json()),
                Err(e) => Ok(json!({"status": "error", "message": e})),
            }
        }

        "skip" => match music_skip(serenity_ctx, data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "leave" => match music_leave(serenity_ctx, data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "pause" => match music_pause(serenity_ctx, data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "resume" => match music_resume(serenity_ctx, data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "volume" => {
            let percent = match params.get("percent").and_then(|v| v.as_u64()) {
                Some(p) if p <= 200 => p as u8,
                _ => {
                    return Ok(json!({
                        "status": "error",
                        "message": "action=volume requires `percent` (integer 0–200)."
                    }));
                }
            };
            match music_volume(serenity_ctx, data, guild_id, percent).await {
                Ok(op) => Ok(op.to_tool_json()),
                Err(e) => Ok(json!({"status": "error", "message": e})),
            }
        }

        "now_playing" => match music_now_playing(serenity_ctx, data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "lyrics" => match music_lyrics(data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "loop" => {
            let mode_s = params
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let Some(mode) = LoopModeArg::parse(mode_s) else {
                return Ok(json!({
                    "status": "error",
                    "message": "action=loop requires `mode`: off, track, or queue."
                }));
            };
            match music_loop(serenity_ctx, data, guild_id, mode).await {
                Ok(op) => Ok(op.to_tool_json()),
                Err(e) => Ok(json!({"status": "error", "message": e})),
            }
        }

        "clear" => match music_clear(serenity_ctx, data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "shuffle" => match music_shuffle(serenity_ctx, data, guild_id).await {
            Ok(op) => Ok(op.to_tool_json()),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        "remove" => {
            let position = match params.get("position").and_then(|v| v.as_u64()) {
                Some(p) if p >= 1 => p as u32,
                _ => {
                    return Ok(json!({
                        "status": "error",
                        "message": "action=remove requires `position` (integer ≥ 1)."
                    }));
                }
            };
            match music_remove(serenity_ctx, data, guild_id, position).await {
                Ok(op) => Ok(op.to_tool_json()),
                Err(e) => Ok(json!({"status": "error", "message": e})),
            }
        }

        "move_track" => {
            let from = match params.get("from").and_then(|v| v.as_u64()) {
                Some(p) if p >= 1 => p as u32,
                _ => {
                    return Ok(json!({
                        "status": "error",
                        "message": "action=move_track requires `from` (integer ≥ 1)."
                    }));
                }
            };
            let to = match params.get("to").and_then(|v| v.as_u64()) {
                Some(p) if p >= 1 => p as u32,
                _ => {
                    return Ok(json!({
                        "status": "error",
                        "message": "action=move_track requires `to` (integer ≥ 1)."
                    }));
                }
            };
            match music_move_track(serenity_ctx, data, guild_id, from, to).await {
                Ok(op) => Ok(op.to_tool_json()),
                Err(e) => Ok(json!({"status": "error", "message": e})),
            }
        }

        "queue" => match music_queue_tool_value(serenity_ctx, data, guild_id).await {
            Ok(v) => Ok(json!({"status": "ok", "action": "queue", "queue": v})),
            Err(e) => Ok(json!({"status": "error", "message": e})),
        },

        _ => Ok(json!({
            "status": "error",
            "message": format!(
                "Unknown action `{action}`. Use help for a list."
            )
        })),
    }
}

#[async_trait]
impl Tool for MusicTool {
    fn name(&self) -> &str {
        MUSIC_AGENT_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Control music in a voice channel: same actions as slash commands (join, play, skip, leave, pause, resume, volume, now_playing, lyrics, loop, clear, shuffle, remove, move_track, queue, help). User must be in a guild; for play/join typically in a voice channel. For best results with play, prefer concrete queries (URL or artist/title) when available. Use action=help for parameters."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "join | play | skip | leave | pause | resume | volume | now_playing | lyrics | loop | clear | shuffle | remove | move_track | queue | help"
                },
                "query": { "type": "string", "description": "Required for play: search text or URL (more specific queries usually return better results)" },
                "playlist": { "type": "boolean", "description": "Optional for play: expand playlists" },
                "percent": { "type": "integer", "description": "Required for volume: 0–200" },
                "mode": { "type": "string", "description": "Required for loop: off | track | queue" },
                "position": { "type": "integer", "description": "Required for remove: 1 = now playing" },
                "from": { "type": "integer", "description": "Required for move_track" },
                "to": { "type": "integer", "description": "Required for move_track" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, _params: Value) -> anyhow::Result<Value> {
        Ok(json!({
            "status": "error",
            "message": "music requires Discord context; mention the bot or reply in a server channel."
        }))
    }

    async fn execute_with_discord(
        &self,
        params: Value,
        dctx: Option<&DiscordToolContext<'_>>,
    ) -> anyhow::Result<Value> {
        let Some(ctx) = dctx else {
            warn!("music: execute_with_discord called without Discord context");
            return self.execute(params).await;
        };
        info!(user = ctx.user_id.get(), "music tool dispatch");
        dispatch_music_tool(
            params,
            ctx.serenity_ctx,
            ctx.data,
            ctx.guild_id,
            ctx.user_id,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::music_ops;

    #[test]
    fn tool_name_matches_ops() {
        let t = MusicTool;
        assert_eq!(t.name(), music_ops::MUSIC_AGENT_TOOL_NAME);
    }
}
