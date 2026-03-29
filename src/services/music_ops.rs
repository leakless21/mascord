//! Unified music operations: same code paths for slash commands, the LLM `music` tool, and tests.
//!
//! Add or change voice/queue behavior here first, then keep Poise handlers and [`crate::tools::builtin::music::MusicTool`] as thin wrappers.

use crate::commands::music::format::{format_hms, format_pos_dur, truncate};
use crate::commands::music::playback::{
    enqueue_playback, join_voice_channel_serenity, EnqueueOpts, TrackUserData,
};
use crate::commands::music::queue_ops::{move_by_adjacent_swaps, shuffle_queue_tail_keep_head};
use crate::commands::music::state::LoopMode;
use crate::Data;
use lavalink_rs::model::GuildId as LavaGuildId;
use poise::serenity_prelude as serenity;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde_json::{json, Value};
use songbird::tracks::{PlayMode, TrackHandle};
use std::time::Duration;
use tracing::info;

/// Single LLM function name for music; mirrors slash music commands (do not add `play_music`, `skip_track`, … duplicates).
pub const MUSIC_AGENT_TOOL_NAME: &str = "music";

pub const MUSIC_HELP_TEXT: &str = "\
Music (guild + voice): use `action` to match slash commands.

- **join** — join your voice channel
- **play** — `query` (search or URL); optional `playlist` (bool) for playlist URLs
- **skip** — skip current track
- **leave** — stop and leave voice
- **pause** / **resume**
- **volume** — `percent` 0–200 (100 = default)
- **now_playing** — current track info
- **lyrics** — lyrics for the current track (requires Lavalink + LavaLyrics on the node)
- **loop** — `mode`: off | track | queue
- **clear** — clear upcoming (keeps current)
- **shuffle** — shuffle upcoming
- **remove** — `position` (1 = now playing)
- **move_track** — `from` and `to` positions (1-based)
- **queue** — list queue (for the model; users can use `/queue` for the interactive embed)
- **help** — this text

The user should be in a voice channel for play/join; the bot needs Connect + Speak.";

/// Outcome shared by slash commands and the `music` tool.
#[derive(Debug, Clone)]
pub enum MusicOp {
    Join {
        channel_id: u64,
    },
    Play {
        added: usize,
        display_title: String,
        query: String,
    },
    Skip,
    Leave,
    Pause,
    Resume,
    Volume {
        percent: u8,
    },
    NowPlaying {
        title: String,
        line: String,
        thumbnail: Option<String>,
    },
    Loop {
        mode: &'static str,
    },
    Clear {
        n: u32,
    },
    Shuffle,
    Remove {
        position: u32,
        skipped_current: bool,
    },
    Move {
        from: u32,
        to: u32,
    },
    /// Lavalink + LavaLyrics (`/v4/lyrics`).
    Lyrics {
        text: String,
        source: Option<String>,
    },
}

impl MusicOp {
    pub fn discord_message(&self) -> String {
        match self {
            MusicOp::Join { channel_id } => format!("🔊 Joined <#{}>", channel_id),
            MusicOp::Play {
                added,
                display_title,
                ..
            } => {
                let title = truncate(display_title, 200);
                if *added > 1 {
                    format!("🎵 Added {} tracks\n**{}**", added, title)
                } else {
                    format!("🎵 Added to Queue\n**{}**", title)
                }
            }
            MusicOp::Skip => "⏭️ Skipped current song".to_string(),
            MusicOp::Leave => "👋 Left voice channel".to_string(),
            MusicOp::Pause => "⏸️ Paused".to_string(),
            MusicOp::Resume => "▶️ Resumed".to_string(),
            MusicOp::Volume { percent } => format!("🔊 Volume set to **{}%**", percent),
            MusicOp::NowPlaying { line, .. } => format!("🎶 Now playing\n{}", line),
            MusicOp::Loop { mode } => match *mode {
                "off" => "🔁 Loop **off**".to_string(),
                "track" => "🔁 Looping **current track**".to_string(),
                "queue" => "🔁 **Queue** will repeat when it finishes.".to_string(),
                _ => "🔁 Loop updated.".to_string(),
            },
            MusicOp::Clear { n } => format!("🗑️ Cleared **{}** upcoming track(s)", n),
            MusicOp::Shuffle => "🔀 Shuffled upcoming tracks".to_string(),
            MusicOp::Remove {
                position,
                skipped_current,
            } => {
                if *skipped_current {
                    "⏭️ Removed current track (skipped)".to_string()
                } else {
                    format!("🗑️ Removed track at position **{}**", position)
                }
            }
            MusicOp::Move { from, to } => format!("↔️ Moved **{}** → **{}**", from, to),
            MusicOp::Lyrics { text, source } => {
                let mut s = String::new();
                if let Some(ref src) = source {
                    s.push_str(&format!("🎤 **{}**\n\n", src));
                } else {
                    s.push_str("🎤 **Lyrics**\n\n");
                }
                s.push_str(text);
                s
            }
        }
    }

    pub fn to_tool_json(&self) -> Value {
        match self {
            MusicOp::Join { channel_id } => {
                json!({"status": "ok", "action": "join", "channel_id": channel_id})
            }
            MusicOp::Play {
                added,
                display_title,
                query,
            } => json!({
                "status": "ok",
                "action": "play",
                "added": added,
                "title": display_title,
                "query": query
            }),
            MusicOp::Skip => json!({"status": "ok", "action": "skip"}),
            MusicOp::Leave => json!({"status": "ok", "action": "leave"}),
            MusicOp::Pause => json!({"status": "ok", "action": "pause"}),
            MusicOp::Resume => json!({"status": "ok", "action": "resume"}),
            MusicOp::Volume { percent } => {
                json!({"status": "ok", "action": "volume", "percent": percent})
            }
            MusicOp::NowPlaying {
                title,
                line,
                thumbnail,
            } => json!({
                "status": "ok",
                "action": "now_playing",
                "title": title,
                "description": line,
                "thumbnail": thumbnail
            }),
            MusicOp::Loop { mode } => json!({"status": "ok", "action": "loop", "mode": mode}),
            MusicOp::Clear { n } => {
                json!({"status": "ok", "action": "clear", "cleared_upcoming": n})
            }
            MusicOp::Shuffle => json!({"status": "ok", "action": "shuffle"}),
            MusicOp::Remove {
                position,
                skipped_current,
            } => json!({
                "status": "ok",
                "action": "remove",
                "position": position,
                "skipped_current": skipped_current
            }),
            MusicOp::Move { from, to } => {
                json!({"status": "ok", "action": "move_track", "from": from, "to": to})
            }
            MusicOp::Lyrics { text, source } => {
                json!({"status": "ok", "action": "lyrics", "text": text, "source": source})
            }
        }
    }
}

pub fn play_embed(op: &MusicOp) -> Option<poise::serenity_prelude::CreateEmbed> {
    use poise::serenity_prelude::CreateEmbed;
    match op {
        MusicOp::Play {
            added,
            display_title,
            ..
        } => {
            let title = truncate(display_title, 200);
            Some(
                CreateEmbed::new()
                    .title(if *added > 1 {
                        format!("🎵 Added {} tracks", added)
                    } else {
                        "🎵 Added to Queue".to_string()
                    })
                    .description(format!("**{}**", title))
                    .color(0x57F287),
            )
        }
        _ => None,
    }
}

/// Embed for the `/play` message after pause/skip (live queue state).
pub async fn play_controls_refresh_embed(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Option<serenity::CreateEmbed> {
    if let Some(ll) = &data.lavalink {
        let p = ll.get_player_context(LavaGuildId(guild_id.get()))?;
        let pl = p.get_player().await.ok()?;
        let t = pl.track?;
        let title = truncate(&t.info.title, 200);
        let pos = std::time::Duration::from_millis(pl.state.position);
        let dur = std::time::Duration::from_millis(t.info.length);
        let paused = pl.paused;
        let line = format_pos_dur(Some(pos), Some(dur));
        let mut desc = format!("**{}**\n{}", title, line);
        if paused {
            desc.push_str("\n\n⏸️ **Paused**");
        }
        let mut e = serenity::CreateEmbed::new()
            .title("🎶 Now playing")
            .description(desc)
            .color(0x57F287);
        if let Some(ref u) = t.info.artwork_url {
            e = e.image(u.clone());
        }
        return Some(e);
    }

    let manager = songbird::get(serenity_ctx).await?;
    let handler_lock = manager.get(guild_id)?;
    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    let all = queue.current_queue();
    if all.is_empty() {
        return None;
    }
    let h = all.first()?;
    let d = h.data::<TrackUserData>();
    let st = h.get_info().await.ok();
    let pos = st.as_ref().map(|s| s.position);
    let paused = st
        .as_ref()
        .map(|s| s.playing == PlayMode::Pause)
        .unwrap_or(false);
    let line = format_pos_dur(pos, d.duration);
    let title = truncate(&d.title, 200);
    let mut desc = format!("**{}**\n{}", title, line);
    if paused {
        desc.push_str("\n\n⏸️ **Paused**");
    }
    let mut e = serenity::CreateEmbed::new()
        .title("🎶 Now playing")
        .description(desc)
        .color(0x57F287);
    if let Some(ref t) = d.thumbnail {
        e = e.image(t);
    }
    Some(e)
}

pub fn now_playing_embed(op: &MusicOp) -> Option<poise::serenity_prelude::CreateEmbed> {
    use poise::serenity_prelude::CreateEmbed;
    match op {
        MusicOp::NowPlaying {
            line, thumbnail, ..
        } => {
            let mut e = CreateEmbed::new()
                .title("🎶 Now playing")
                .description(line.clone())
                .color(0x5865F2);
            if let Some(ref t) = thumbnail {
                e = e.image(t);
            }
            Some(e)
        }
        _ => None,
    }
}

/// One page of the queue embed (slash `/queue` and button refresh).
pub struct QueuePageBuilt {
    /// Now playing + up next lines only (`loop_note` and total are appended by the caller).
    pub body: String,
    pub total_dur: Duration,
    pub total_pages: usize,
    pub upcoming_only_len: usize,
    pub loop_note: &'static str,
}

pub fn loop_note_for_embed(loop_mode: LoopMode) -> &'static str {
    match loop_mode {
        LoopMode::Off => "",
        LoopMode::Track => "\n🔁 Track loop on",
        LoopMode::Queue => "\n🔁 Queue loop on",
    }
}

pub async fn build_queue_page(
    tracks: &[TrackHandle],
    page: usize,
    items_per_page: usize,
    loop_mode: LoopMode,
) -> QueuePageBuilt {
    let mut total_dur = Duration::ZERO;
    for h in tracks {
        if let Some(d) = h.data::<TrackUserData>().duration {
            total_dur += d;
        }
    }

    let upcoming_only_len = tracks.len().saturating_sub(1);
    let total_pages = if upcoming_only_len == 0 {
        1usize
    } else {
        (upcoming_only_len as f32 / items_per_page as f32).ceil() as usize
    };

    let loop_note = loop_note_for_embed(loop_mode);

    let mut body = String::new();
    if let Some(h) = tracks.first() {
        let d = h.data::<TrackUserData>();
        let st = h.get_info().await.ok();
        let pos = st.as_ref().map(|s| s.position);
        body.push_str("**Now playing:**\n");
        body.push_str(&format!(
            "🎶 **{}**\n{}\n\n",
            truncate(&d.title, 80),
            format_pos_dur(pos, d.duration)
        ));
    }

    let start = page * items_per_page;
    let end = (start + items_per_page).min(upcoming_only_len);
    if upcoming_only_len > 0 {
        body.push_str("**Up next:**\n");
        for i in start..end {
            let idx = i + 1;
            let h = &tracks[idx];
            let d = h.data::<TrackUserData>();
            let dur = d.duration.map(format_hms).unwrap_or_else(|| "?".into());
            body.push_str(&format!(
                "{}. {} — `{}`\n",
                idx + 1,
                truncate(&d.title, 60),
                dur
            ));
        }
    } else {
        body.push_str("*Nothing else queued*");
    }

    QueuePageBuilt {
        body,
        total_dur,
        total_pages,
        upcoming_only_len,
        loop_note,
    }
}

/// `/queue` embed for Lavalink (current track + Lavalink queue).
pub async fn build_queue_page_lavalink(
    data: &Data,
    guild_id: serenity::GuildId,
    page: usize,
    items_per_page: usize,
) -> Result<QueuePageBuilt, String> {
    let ll = data
        .lavalink
        .as_ref()
        .ok_or_else(|| "Lavalink not active".to_string())?;
    let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
        return Err("Not in voice".to_string());
    };
    let pl = p.get_player().await.map_err(|e| e.to_string())?;
    let dq = p
        .get_queue()
        .get_queue()
        .await
        .map_err(|e| e.to_string())?;
    let loop_mode = data.music.loop_mode(guild_id.get());

    let mut total_dur = Duration::ZERO;
    if let Some(ref t) = pl.track {
        total_dur += Duration::from_millis(t.info.length);
    }
    for x in &dq {
        total_dur += Duration::from_millis(x.track.info.length);
    }

    let upcoming_only_len = dq.len();
    let total_pages = if upcoming_only_len == 0 {
        1usize
    } else {
        (upcoming_only_len as f32 / items_per_page as f32).ceil() as usize
    };

    let loop_note = loop_note_for_embed(loop_mode);

    let mut body = String::new();
    if let Some(ref t) = pl.track {
        let pos = Duration::from_millis(pl.state.position);
        let dur = Duration::from_millis(t.info.length);
        body.push_str("**Now playing:**\n");
        body.push_str(&format!(
            "🎶 **{}**\n{}\n\n",
            truncate(&t.info.title, 80),
            format_pos_dur(Some(pos), Some(dur))
        ));
    } else {
        body.push_str("*Nothing playing*\n\n");
    }

    let start = page * items_per_page;
    let end = (start + items_per_page).min(upcoming_only_len);
    if upcoming_only_len > 0 {
        body.push_str("**Up next:**\n");
        for (i, x) in dq
            .iter()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
        {
            let pos = if pl.track.is_some() {
                i + 2
            } else {
                i + 1
            };
            let dur = format_hms(Duration::from_millis(x.track.info.length));
            body.push_str(&format!(
                "{}. {} — `{}`\n",
                pos,
                truncate(&x.track.info.title, 60),
                dur
            ));
        }
    } else {
        body.push_str("*Nothing else queued*");
    }

    Ok(QueuePageBuilt {
        body,
        total_dur,
        total_pages,
        upcoming_only_len,
        loop_note,
    })
}

pub async fn lavalink_queue_is_empty(data: &Data, guild_id: serenity::GuildId) -> bool {
    let Some(ll) = &data.lavalink else {
        return true;
    };
    let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
        return true;
    };
    let Ok(pl) = p.get_player().await else {
        return true;
    };
    let Ok(dq) = p.get_queue().get_queue().await else {
        return true;
    };
    pl.track.is_none() && dq.is_empty()
}

pub async fn music_join(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
) -> Result<MusicOp, String> {
    let channel_id = join_voice_channel_serenity(
        serenity_ctx,
        data,
        guild_id,
        user_id,
        &data.music,
        data.http_client.clone(),
        data.config.youtube_cookies.clone(),
    )
    .await?;
    Ok(MusicOp::Join {
        channel_id: channel_id.get(),
    })
}

pub async fn music_play(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    query: String,
    expand_playlist: bool,
) -> Result<MusicOp, String> {
    let summary = enqueue_playback(
        serenity_ctx,
        data,
        guild_id,
        user_id,
        query.clone(),
        &data.music,
        EnqueueOpts {
            expand_playlist,
            skip_voice_check: false,
        },
    )
    .await?;
    let display_title = summary.first_title.unwrap_or_else(|| query.clone());
    Ok(MusicOp::Play {
        added: summary.added,
        display_title,
        query,
    })
}

pub async fn music_skip(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ I'm not in a voice channel".to_string());
        };
        let n = p.get_queue().get_count().await.map_err(|e| e.to_string())?;
        let has_cur = p
            .get_player()
            .await
            .map_err(|e| e.to_string())?
            .track
            .is_some();
        if !has_cur && n == 0 {
            return Err("📭 Queue is empty".to_string());
        }
        info!("Skip in guild {}: skipping current song (Lavalink)", guild_id);
        p.skip().map_err(|e| e.to_string())?;
        return Ok(MusicOp::Skip);
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird Voice client not initialized".to_string())?;
    let Some(handler_lock) = manager.get(guild_id) else {
        return Err("❌ I'm not in a voice channel".to_string());
    };
    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    if queue.is_empty() {
        return Err("📭 Queue is empty".to_string());
    }
    info!("Skip in guild {}: skipping current song", guild_id);
    queue.skip().map_err(|e| e.to_string())?;
    Ok(MusicOp::Skip)
}

pub async fn music_leave(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird Voice client not initialized".to_string())?;
    if manager.get(guild_id).is_none() {
        return Err("❌ I'm not in a voice channel".to_string());
    }
    info!("Leave: removing voice handler for guild {}", guild_id);
    data.music.cancel_alone_leave_task(guild_id.get());
    data.music.clear_voice_hooks(guild_id.get());
    if let Some(ll) = &data.lavalink {
        let _ = ll.delete_player(LavaGuildId(guild_id.get())).await;
    }
    manager.remove(guild_id).await.map_err(|e| e.to_string())?;
    Ok(MusicOp::Leave)
}

pub async fn music_pause(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ Not in voice".to_string());
        };
        p.set_pause(true).await.map_err(|e| e.to_string())?;
        return Ok(MusicOp::Pause);
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.pause().map_err(|e| e.to_string())?;
    Ok(MusicOp::Pause)
}

pub async fn music_resume(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ Not in voice".to_string());
        };
        p.set_pause(false).await.map_err(|e| e.to_string())?;
        return Ok(MusicOp::Resume);
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.resume().map_err(|e| e.to_string())?;
    Ok(MusicOp::Resume)
}

pub async fn music_volume(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    percent: u8,
) -> Result<MusicOp, String> {
    let v = (percent as f32 / 100.0).clamp(0.0, 2.0);
    data.music.set_volume(guild_id.get(), v);
    if let Some(ll) = &data.lavalink {
        if let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) {
            let vol_u16 = (percent as u32).min(1000) as u16;
            let _ = p.set_volume(vol_u16).await;
        }
        return Ok(MusicOp::Volume { percent });
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    if let Some(lock) = manager.get(guild_id) {
        let handler = lock.lock().await;
        let q = handler.queue();
        for h in q.current_queue() {
            let _ = h.set_volume(v);
        }
    }
    Ok(MusicOp::Volume { percent })
}

fn format_lavalyrics_json(v: &serde_json::Value) -> (String, Option<String>) {
    let source = v
        .get("provider")
        .or_else(|| v.get("sourceName"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let mut out = String::new();
    if let Some(lines) = v.get("lines").and_then(|l| l.as_array()) {
        for line in lines.iter().take(45) {
            if let Some(t) = line.get("line").and_then(|x| x.as_str()) {
                if !t.is_empty() {
                    out.push_str(t);
                    out.push('\n');
                }
            }
        }
    }
    if out.is_empty() {
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
            out = t.to_string();
        }
    }
    (out.trim().to_string(), source)
}

/// Fetches lyrics for the current track via LavaLyrics (`GET /v4/lyrics`).
pub async fn music_lyrics(
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    if data.lavalink.is_none() {
        return Err(
            "Lyrics need Lavalink with LavaLyrics on the node (LAVALINK_ENABLED=true).".into(),
        );
    }
    let ll = data.lavalink.as_ref().unwrap();
    let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
        return Err("❌ Not in voice".to_string());
    };
    let pl = p.get_player().await.map_err(|e| e.to_string())?;
    let Some(t) = pl.track else {
        return Err("📭 Nothing playing".to_string());
    };
    let encoded = &t.encoded;
    let j = crate::services::lavalink_rest::fetch_lyrics_for_encoded_track(
        &data.http_client,
        &data.config.lavalink_host,
        &data.config.lavalink_password,
        encoded,
    )
    .await?;
    let Some(v) = j else {
        return Err("No lyrics found for this track.".to_string());
    };
    let (body, src) = format_lavalyrics_json(&v);
    if body.is_empty() {
        return Err("No lyrics found for this track.".to_string());
    }
    let text = truncate(&body, 1900).to_string();
    Ok(MusicOp::Lyrics {
        text,
        source: src,
    })
}

pub async fn music_now_playing(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ Not in voice".to_string());
        };
        let pl = p.get_player().await.map_err(|e| e.to_string())?;
        let Some(t) = pl.track else {
            return Err("📭 Nothing playing".to_string());
        };
        let title = t.info.title.clone();
        let pos = std::time::Duration::from_millis(pl.state.position);
        let dur = std::time::Duration::from_millis(t.info.length);
        let line = format!("**{}**\n{}", title, format_pos_dur(Some(pos), Some(dur)));
        return Ok(MusicOp::NowPlaying {
            title,
            line,
            thumbnail: t.info.artwork_url.clone(),
        });
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let q = handler.queue();
    let Some(cur) = q.current() else {
        return Err("📭 Nothing playing".to_string());
    };
    let ud = cur.data::<TrackUserData>();
    let state = cur.get_info().await.ok();
    let pos = state.as_ref().map(|s| s.position);
    let dur = ud.duration;
    let line = format!("**{}**\n{}", ud.title, format_pos_dur(pos, dur));
    Ok(MusicOp::NowPlaying {
        title: ud.title.clone(),
        line,
        thumbnail: ud.thumbnail.clone(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopModeArg {
    Off,
    Track,
    Queue,
}

impl LoopModeArg {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "track" => Some(Self::Track),
            "queue" => Some(Self::Queue),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            LoopModeArg::Off => "off",
            LoopModeArg::Track => "track",
            LoopModeArg::Queue => "queue",
        }
    }
}

pub async fn music_loop(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    mode: LoopModeArg,
) -> Result<MusicOp, String> {
    if data.lavalink.is_some() && mode != LoopModeArg::Off {
        return Err(
            "🔁 Loop track/queue is not implemented for Lavalink yet. Set LAVALINK_ENABLED=false or use loop off."
                .to_string(),
        );
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    match mode {
        LoopModeArg::Off => {
            data.music.set_loop_mode(guild_id.get(), LoopMode::Off);
            if let Some(h) = queue.current() {
                let _ = h.disable_loop();
            }
            Ok(MusicOp::Loop {
                mode: LoopModeArg::Off.as_str(),
            })
        }
        LoopModeArg::Track => {
            data.music.set_loop_mode(guild_id.get(), LoopMode::Track);
            if let Some(h) = queue.current() {
                let _ = h.disable_loop();
                let _ = h.enable_loop();
            }
            Ok(MusicOp::Loop {
                mode: LoopModeArg::Track.as_str(),
            })
        }
        LoopModeArg::Queue => {
            let handles = queue.current_queue();
            let snapshot: Vec<(String, bool)> = handles
                .iter()
                .map(|h| {
                    let u = h.data::<TrackUserData>();
                    let is_url =
                        u.source.starts_with("http://") || u.source.starts_with("https://");
                    (u.source.clone(), is_url)
                })
                .collect();
            if snapshot.is_empty() {
                return Err("📭 Queue is empty — nothing to loop.".to_string());
            }
            drop(handler);
            data.music.set_queue_snapshot(guild_id.get(), snapshot);
            Ok(MusicOp::Loop {
                mode: LoopModeArg::Queue.as_str(),
            })
        }
    }
}

pub async fn music_clear(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ Not in voice".to_string());
        };
        let q = p.get_queue();
        let n = q.get_count().await.map_err(|e| e.to_string())?;
        q.clear().map_err(|e| e.to_string())?;
        return Ok(MusicOp::Clear { n: n as u32 });
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    let mut n = 0u32;
    queue.modify_queue(|q| {
        while q.len() > 1 {
            if let Some(t) = q.pop_back() {
                let _ = t.stop();
                n += 1;
            }
        }
    });
    Ok(MusicOp::Clear { n })
}

pub async fn music_shuffle(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<MusicOp, String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ Not in voice".to_string());
        };
        let q = p.get_queue();
        let dq = q.get_queue().await.map_err(|e| e.to_string())?;
        if dq.len() <= 1 {
            return Ok(MusicOp::Shuffle);
        }
        let mut v: Vec<_> = dq.into_iter().collect();
        v.shuffle(&mut thread_rng());
        let mut dq = std::collections::VecDeque::new();
        for t in v {
            dq.push_back(t);
        }
        q.replace(dq).map_err(|e| e.to_string())?;
        return Ok(MusicOp::Shuffle);
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.modify_queue(|q| shuffle_queue_tail_keep_head(q, &mut thread_rng()));
    Ok(MusicOp::Shuffle)
}

pub async fn music_remove(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    position: u32,
) -> Result<MusicOp, String> {
    if position == 0 {
        return Err("❌ Position must be at least 1".to_string());
    }

    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ Not in voice".to_string());
        };
        let q = p.get_queue();
        let n = q.get_count().await.map_err(|e| e.to_string())?;
        let has_cur = p
            .get_player()
            .await
            .map_err(|e| e.to_string())?
            .track
            .is_some();
        let total = n + if has_cur { 1 } else { 0 };
        if position as usize > total {
            return Err("❌ Invalid position".to_string());
        }
        if position == 1 {
            p.skip().map_err(|e| e.to_string())?;
            return Ok(MusicOp::Remove {
                position,
                skipped_current: true,
            });
        }
        let qi = (position - 2) as usize;
        if qi >= n {
            return Err("❌ Invalid position".to_string());
        }
        q.remove(qi).map_err(|e| e.to_string())?;
        return Ok(MusicOp::Remove {
            position,
            skipped_current: false,
        });
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    let idx = (position - 1) as usize;
    if idx >= queue.len() {
        return Err("❌ Invalid position".to_string());
    }
    if idx == 0 {
        queue.skip().map_err(|e| e.to_string())?;
        Ok(MusicOp::Remove {
            position,
            skipped_current: true,
        })
    } else if let Some(t) = queue.dequeue(idx) {
        let _ = t.stop();
        Ok(MusicOp::Remove {
            position,
            skipped_current: false,
        })
    } else {
        Err("❌ Invalid position".to_string())
    }
}

pub async fn music_move_track(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    from: u32,
    to: u32,
) -> Result<MusicOp, String> {
    if from == 0 || to == 0 {
        return Err("❌ Positions must be ≥ 1".to_string());
    }
    let from_i = (from - 1) as usize;
    let to_i = (to - 1) as usize;
    if from_i == to_i {
        return Err("Nothing to do.".to_string());
    }

    if data.lavalink.is_some() {
        return Err(
            "↔️ Move is not implemented for Lavalink yet. Re-add tracks or use the yt-dlp path."
                .to_string(),
        );
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(lock) = manager.get(guild_id) else {
        return Err("❌ Not in voice".to_string());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.modify_queue(|q| move_by_adjacent_swaps(q, from_i, to_i));
    Ok(MusicOp::Move { from, to })
}

/// JSON snapshot of the queue for the LLM (`action=queue`).
pub async fn music_queue_tool_value(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<Value, String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("❌ I'm not in a voice channel".to_string());
        };
        let pl = p.get_player().await.map_err(|e| e.to_string())?;
        let q = p.get_queue();
        let dq = q.get_queue().await.map_err(|e| e.to_string())?;
        let loop_mode = loop_mode_str(data.music.loop_mode(guild_id.get()));
        if pl.track.is_none() && dq.is_empty() {
            return Ok(json!({
                "empty": true,
                "loop_mode": loop_mode,
            }));
        }
        let mut total_dur = Duration::ZERO;
        if let Some(ref t) = pl.track {
            total_dur += Duration::from_millis(t.info.length);
        }
        for x in &dq {
            total_dur += Duration::from_millis(x.track.info.length);
        }
        let mut now = serde_json::Map::new();
        if let Some(ref t) = pl.track {
            let pos = Duration::from_millis(pl.state.position);
            let dur = Duration::from_millis(t.info.length);
            now.insert("position".into(), json!(1));
            now.insert("title".into(), json!(t.info.title));
            now.insert("line".into(), json!(format_pos_dur(Some(pos), Some(dur))));
        }
        let mut upcoming = Vec::new();
        for (i, x) in dq.iter().enumerate() {
            let dur = format_hms(Duration::from_millis(x.track.info.length));
            upcoming.push(json!({
                "position": i + 2,
                "title": x.track.info.title,
                "duration": dur
            }));
        }
        return Ok(json!({
            "empty": false,
            "loop_mode": loop_mode,
            "total_est_hms": format_hms(total_dur),
            "now_playing": Value::Object(now),
            "upcoming": upcoming
        }));
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird Voice client not initialized".to_string())?;
    let Some(handler_lock) = manager.get(guild_id) else {
        return Err("❌ I'm not in a voice channel".to_string());
    };
    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    let all = queue.current_queue();
    if all.is_empty() {
        return Ok(json!({
            "empty": true,
            "loop_mode": loop_mode_str(data.music.loop_mode(guild_id.get())),
        }));
    }

    let loop_mode = loop_mode_str(data.music.loop_mode(guild_id.get()));
    let mut total_dur = Duration::ZERO;
    for h in &all {
        if let Some(d) = h.data::<TrackUserData>().duration {
            total_dur += d;
        }
    }

    let mut now = serde_json::Map::new();
    if let Some(h) = all.first() {
        let d = h.data::<TrackUserData>();
        let st = h.get_info().await.ok();
        let pos = st.as_ref().map(|s| s.position);
        now.insert("position".into(), json!(1));
        now.insert("title".into(), json!(d.title));
        now.insert("line".into(), json!(format_pos_dur(pos, d.duration)));
    }

    let mut upcoming = Vec::new();
    for (i, h) in all.iter().enumerate().skip(1) {
        let d = h.data::<TrackUserData>();
        let dur = d.duration.map(format_hms).unwrap_or_else(|| "?".into());
        upcoming.push(json!({
            "position": i + 1,
            "title": d.title,
            "duration": dur
        }));
    }

    Ok(json!({
        "empty": false,
        "loop_mode": loop_mode,
        "total_est_hms": format_hms(total_dur),
        "now_playing": Value::Object(now),
        "upcoming": upcoming
    }))
}

fn loop_mode_str(m: LoopMode) -> &'static str {
    match m {
        LoopMode::Off => "off",
        LoopMode::Track => "track",
        LoopMode::Queue => "queue",
    }
}

/// Stop playback, clear hooks, leave — used by `/queue` message **Stop** button.
pub async fn music_stop_and_leave(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<(), String> {
    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird Voice client not initialized".to_string())?;
    let Some(handler_lock) = manager.get(guild_id) else {
        return Err("Not in voice".to_string());
    };
    if let Some(ll) = &data.lavalink {
        if let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) {
            let _ = p.stop_now().await;
        }
        let _ = ll.delete_player(LavaGuildId(guild_id.get())).await;
    }
    let mut handler = handler_lock.lock().await;
    let queue = handler.queue();
    queue.stop();
    data.music.cancel_alone_leave_task(guild_id.get());
    data.music.clear_voice_hooks(guild_id.get());
    handler.leave().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Pause or resume — used by `/queue` **Pause** button.
pub async fn music_toggle_pause(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<(), String> {
    if let Some(ll) = &data.lavalink {
        let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
            return Err("Not in voice".to_string());
        };
        let pl = p.get_player().await.map_err(|e| e.to_string())?;
        let pause = !pl.paused;
        p.set_pause(pause).await.map_err(|e| e.to_string())?;
        return Ok(());
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(handler_lock) = manager.get(guild_id) else {
        return Err("Not in voice".to_string());
    };
    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    if let Some(cur) = queue.current() {
        if let Ok(st) = cur.get_info().await {
            if st.playing == PlayMode::Pause {
                let _ = queue.resume();
            } else {
                let _ = queue.pause();
            }
        }
    }
    Ok(())
}

pub async fn music_skip_button(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<(), String> {
    if let Some(ll) = &data.lavalink {
        if let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) {
            let _ = p.skip();
        }
        return Ok(());
    }

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized".to_string())?;
    let Some(handler_lock) = manager.get(guild_id) else {
        return Ok(());
    };
    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    let _ = queue.skip();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_tool_name_is_music() {
        assert_eq!(MUSIC_AGENT_TOOL_NAME, "music");
    }

    #[test]
    fn loop_mode_arg_parses() {
        assert_eq!(LoopModeArg::parse("OFF"), Some(LoopModeArg::Off));
        assert_eq!(LoopModeArg::parse("Queue"), Some(LoopModeArg::Queue));
        assert!(LoopModeArg::parse("nope").is_none());
    }
}
