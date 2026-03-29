//! Shared YouTube / yt-dlp enqueue logic for slash commands and agent tools.

use crate::commands::music::state::MusicState;
use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use songbird::input::{Compose, YoutubeDl};
use songbird::tracks::{PlayMode, Track, TrackHandle};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// User-facing metadata carried on every queued [`songbird::tracks::TrackHandle`].
#[derive(Debug, Clone)]
pub struct TrackUserData {
    pub title: String,
    pub duration: Option<Duration>,
    pub source: String,
    pub thumbnail: Option<String>,
}

pub const MAX_PLAYLIST_ENTRIES: usize = 50;

/// Current + waiting tracks (Songbird queue + active track if any).
pub(crate) fn queued_track_count(handler: &songbird::Call) -> usize {
    let q = handler.queue();
    let mut n = q.current_queue().len();
    if q.current().is_some() {
        n += 1;
    }
    n
}

pub(crate) fn normalize_queue_url(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Whether `source` (normalized) is already the current or queued track.
pub(crate) fn queue_contains_url_source(handler: &songbird::Call, source: &str) -> bool {
    let needle = normalize_queue_url(source);
    let q = handler.queue();
    if let Some(cur) = q.current() {
        let ud = cur.data::<TrackUserData>();
        if normalize_queue_url(&ud.source) == needle {
            return true;
        }
    }
    for h in q.current_queue() {
        let ud = h.data::<TrackUserData>();
        if normalize_queue_url(&ud.source) == needle {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy)]
pub struct EnqueueOpts {
    /// When true and input is an HTTP(S) URL, expand YouTube playlists via yt-dlp.
    pub expand_playlist: bool,
    /// Skip "requester must be in voice" check (queue-loop replay).
    pub skip_voice_check: bool,
}

#[derive(Debug)]
pub struct EnqueueSummary {
    pub added: usize,
    pub first_title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtdlJsonLine {
    webpage_url: Option<String>,
    url: Option<String>,
    title: Option<String>,
    duration: Option<f64>,
    #[serde(rename = "_type")]
    ytdl_type: Option<String>,
}

fn cookies_ok(cookies_path: Option<&String>) -> bool {
    cookies_path
        .map(|p| Path::new(p.as_str()).exists())
        .unwrap_or(false)
}

fn ytdl_user_args(cookies_path: Option<&String>, no_playlist: bool) -> Vec<String> {
    let mut args = Vec::new();
    // Prefer streams Symphonia can probe (WebM/Opus, MP4/AAC); reduces decode failures.
    args.push("-f".to_string());
    args.push("ba[ext=webm]/ba[ext=m4a]/ba/best".to_string());
    if no_playlist {
        args.push("--no-playlist".to_string());
    }
    if let (Some(path), true) = (cookies_path, cookies_ok(cookies_path)) {
        args.push("--cookies".to_string());
        args.push(path.clone());
    }
    args
}

fn display_title_from_aux(
    meta: &songbird::input::AuxMetadata,
    fallback: &str,
) -> (String, Option<Duration>, Option<String>) {
    let title = meta
        .title
        .clone()
        .or_else(|| meta.track.clone())
        .unwrap_or_else(|| fallback.to_string());
    let thumb = meta.thumbnail.clone();
    (title, meta.duration, thumb)
}

/// Fetch playlist or single-video entries using `yt-dlp -j --flat-playlist`.
pub async fn fetch_playlist_entries(
    url: &str,
    cookies_path: Option<&String>,
) -> Result<Vec<(String, Option<String>, Option<Duration>)>, String> {
    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.arg("-j")
        .arg("--no-warnings")
        .arg("--flat-playlist")
        .arg(url);
    if let (Some(path), true) = (cookies_path, cookies_ok(cookies_path)) {
        cmd.arg("--cookies").arg(path);
    }
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("yt-dlp failed: {}", err.trim()));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: YtdlJsonLine =
            serde_json::from_str(line).map_err(|e| format!("Invalid yt-dlp JSON line: {}", e))?;
        if matches!(row.ytdl_type.as_deref(), Some("playlist")) {
            continue;
        }
        let page = row
            .webpage_url
            .or(row.url)
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"));
        let Some(page) = page else {
            continue;
        };
        let dur = row
            .duration
            .filter(|d| d.is_finite() && *d >= 0.0)
            .map(Duration::from_secs_f64);
        entries.push((page, row.title, dur));
        if entries.len() >= MAX_PLAYLIST_ENTRIES {
            break;
        }
    }
    if entries.is_empty() {
        return Err("No playable entries found for that playlist or URL.".into());
    }
    Ok(entries)
}

fn build_ytdl_input(
    http: reqwest::Client,
    item: String,
    is_url: bool,
    cookies_path: Option<&String>,
    no_playlist: bool,
) -> YoutubeDl<'static> {
    let mut src = if is_url {
        YoutubeDl::new(http, item)
    } else {
        YoutubeDl::new_search(http, item)
    };
    src = src.user_args(ytdl_user_args(cookies_path, no_playlist));
    src
}

async fn preflight_source(
    source: YoutubeDl<'_>,
    cookies_ok_flag: bool,
) -> Result<songbird::input::AuxMetadata, String> {
    let mut source = source;
    match source.aux_metadata().await {
        Ok(m) => Ok(m),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            let looks_age_restricted = msg.contains("confirm your age")
                || msg.contains("age-restricted")
                || msg.contains("sign in to confirm your age")
                || msg.contains("age restricted");
            if looks_age_restricted && !cookies_ok_flag {
                return Err(
                    "This video appears to be age-restricted. Configure YOUTUBE_COOKIES.".into(),
                );
            }
            Err(format!("Failed to fetch audio metadata: {}", e))
        }
    }
}

/// After enqueue, Songbird may still fail while probing/decoding. Wait until the track
/// starts playing or errors (only safe when this track will start immediately — e.g. queue was empty).
async fn wait_for_immediate_track_play_or_error(
    handle: &TrackHandle,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(
                "Timed out waiting for audio to start; check yt-dlp, network, and codec support."
                    .into(),
            );
        }
        match handle.get_info().await {
            Ok(state) => {
                match &state.playing {
                    PlayMode::Errored(e) => {
                        return Err(format!(
                            "Audio could not be played: {}. Try another URL or update yt-dlp.",
                            e
                        ));
                    }
                    PlayMode::Play => return Ok(()),
                    PlayMode::End | PlayMode::Stop => {
                        return Err(
                            "Track stopped before playback started (decode or source error)."
                                .into(),
                        );
                    }
                    PlayMode::Pause => {}
                    _ => {}
                }
            }
            Err(e) => {
                return Err(format!(
                    "Lost audio track before playback started: {}",
                    e
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(90)).await;
    }
}

fn meta_to_user_data(
    meta: songbird::input::AuxMetadata,
    source_label: String,
) -> Arc<TrackUserData> {
    let (title, duration, thumbnail) = display_title_from_aux(&meta, &source_label);
    Arc::new(TrackUserData {
        title,
        duration,
        source: source_label,
        thumbnail,
    })
}

/// Enqueue one item (URL or search query). Preflights metadata unless `preflight` is false.
pub async fn enqueue_one(
    handler: &mut songbird::Call,
    http: reqwest::Client,
    item: String,
    is_url: bool,
    cookies_path: Option<String>,
    volume: f32,
    preflight: bool,
    verify_immediate_decode: bool,
) -> Result<TrackUserData, String> {
    let cookies_ref = cookies_path.as_ref();
    let cookies_ok_flag = cookies_ok(cookies_ref);
    if cookies_path.is_some() && !cookies_ok_flag {
        warn!("YOUTUBE_COOKIES set but file not found; proceeding without cookies");
    }
    let mut source = build_ytdl_input(http.clone(), item.clone(), is_url, cookies_ref, true);
    let meta = if preflight {
        preflight_source(source.clone(), cookies_ok_flag).await?
    } else {
        source.aux_metadata().await.map_err(|e| e.to_string())?
    };
    let user_data = meta_to_user_data(meta, item.clone());
    let out = (*user_data).clone();
    let input: songbird::input::Input = source.into();
    let track = Track::new_with_data(input, user_data).volume(volume);
    let handle = handler.enqueue(track).await;
    if verify_immediate_decode {
        wait_for_immediate_track_play_or_error(&handle, Duration::from_secs(60)).await?;
    }
    Ok(out)
}

/// Join requester's voice channel and attach idle + queue-loop handlers once per session.
pub async fn join_voice_channel_serenity(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    music: &Arc<MusicState>,
    http_client: reqwest::Client,
    youtube_cookies: Option<String>,
) -> Result<serenity::ChannelId, String> {
    let channel_id = {
        let guild = serenity_ctx
            .cache
            .guild(guild_id)
            .ok_or_else(|| "Guild not in cache — try again shortly.".to_string())?;
        guild
            .voice_states
            .get(&user_id)
            .and_then(|vs| vs.channel_id)
            .ok_or_else(|| "You must be in a voice channel.".to_string())?
    };

    info!("Joining voice channel {} in guild {}", channel_id, guild_id);

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized.".to_string())?
        .clone();

    match manager.join(guild_id, channel_id).await {
        Ok(handler_lock) => {
            let mut handler = handler_lock.lock().await;
            if music.try_install_voice_hooks(guild_id.get()) {
                let idle_timeout_secs = data
                    .db
                    .get_guild_voice_idle_timeout(guild_id.get())
                    .map_err(|e| e.to_string())?
                    .unwrap_or(data.config.voice_idle_timeout_secs);

                handler.add_global_event(
                    songbird::Event::Track(songbird::TrackEvent::End),
                    crate::voice::events::IdleHandler {
                        guild_id,
                        manager: manager.clone(),
                        idle_timeout_secs,
                        music: Arc::clone(music),
                    },
                );

                handler.add_global_event(
                    songbird::Event::Track(songbird::TrackEvent::End),
                    crate::commands::music::handlers::QueueLoopHandler {
                        guild_id,
                        manager: manager.clone(),
                        music: Arc::clone(music),
                        http_client,
                        youtube_cookies,
                        max_queue_tracks: data.config.max_queue_tracks,
                        voice_allow_duplicate_urls: data.config.voice_allow_duplicate_urls,
                    },
                );

                handler.add_global_event(
                    songbird::Event::Track(songbird::TrackEvent::Error),
                    crate::voice::events::TrackErrorHandler {
                        guild_id,
                        manager: manager.clone(),
                    },
                );

                for core in [
                    songbird::CoreEvent::DriverDisconnect,
                    songbird::CoreEvent::DriverReconnect,
                    songbird::CoreEvent::DriverConnect,
                ] {
                    handler.add_global_event(
                        songbird::Event::Core(core),
                        crate::voice::events::DriverLifecycleHandler {
                            guild_id,
                            music: Arc::clone(music),
                        },
                    );
                }
            }
            Ok(channel_id)
        }
        Err(e) => Err(format!("Failed to join voice channel: {}", e)),
    }
}

/// High-level play: ensure voice, then enqueue query or expanded playlist.
pub async fn enqueue_playback(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    requester: serenity::UserId,
    query: String,
    music: &Arc<MusicState>,
    opts: EnqueueOpts,
) -> Result<EnqueueSummary, String> {
    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized.".to_string())?
        .clone();

    let handler_lock = if let Some(h) = manager.get(guild_id) {
        h
    } else {
        if opts.skip_voice_check {
            return Err("Bot is not in a voice channel.".into());
        }
        join_voice_channel_serenity(
            serenity_ctx,
            data,
            guild_id,
            requester,
            music,
            data.http_client.clone(),
            data.config.youtube_cookies.clone(),
        )
        .await?;
        manager
            .get(guild_id)
            .ok_or_else(|| "Voice handler missing after join.".to_string())?
    };

    let mut handler = handler_lock.lock().await;
    let cookies_path = data.config.youtube_cookies.clone();
    let cookies_ref = cookies_path.as_ref();
    let vol = music.volume_for_guild(guild_id.get());
    let is_url = query.starts_with("http://") || query.starts_with("https://");
    let max_q = data.config.max_queue_tracks;
    let current_count = queued_track_count(&handler);

    if is_url && opts.expand_playlist {
        let queue_was_empty = handler.queue().is_empty();
        let mut entries = fetch_playlist_entries(&query, cookies_ref).await?;
        if !data.config.voice_allow_duplicate_urls {
            entries.retain(|(url, _, _)| !queue_contains_url_source(&handler, url));
        }
        if entries.is_empty() {
            return Err("All playlist entries were already in the queue.".into());
        }
        if max_q > 0 {
            let slots = max_q.saturating_sub(current_count);
            if slots == 0 {
                return Err(format!("Queue is full (max {} tracks).", max_q));
            }
            if entries.len() > slots {
                warn!(
                    "Playlist truncated: {} → {} tracks (queue capacity)",
                    entries.len(),
                    slots
                );
                entries.truncate(slots);
            }
        }
        let mut first_title_out = None;
        let mut count = 0usize;
        let ck = cookies_ok(cookies_ref);
        for (i, (url, title_hint, dur_hint)) in entries.into_iter().enumerate() {
            let source = build_ytdl_input(
                data.http_client.clone(),
                url.clone(),
                true,
                cookies_ref,
                true,
            );
            let user_data: Arc<TrackUserData>;
            let input: songbird::input::Input;
            if i == 0 {
                let meta = preflight_source(source.clone(), ck).await?;
                user_data = meta_to_user_data(meta, url.clone());
                input = source.into();
            } else if let Some(t) = title_hint {
                user_data = Arc::new(TrackUserData {
                    title: t,
                    duration: dur_hint,
                    source: url.clone(),
                    thumbnail: None,
                });
                input = source.into();
            } else {
                let mut s = source;
                let meta = s.aux_metadata().await.map_err(|e| e.to_string())?;
                user_data = meta_to_user_data(meta, url.clone());
                input = s.into();
            }
            let track = Track::new_with_data(input, user_data.clone()).volume(vol);
            let h = handler.enqueue(track).await;
            if queue_was_empty && i == 0 {
                wait_for_immediate_track_play_or_error(&h, Duration::from_secs(60)).await?;
            }
            count += 1;
            if first_title_out.is_none() {
                first_title_out = Some(user_data.title.clone());
            }
        }
        info!(
            "Queued {} tracks from playlist in guild {}",
            count, guild_id
        );
        return Ok(EnqueueSummary {
            added: count,
            first_title: first_title_out,
        });
    }

    if max_q > 0 && current_count + 1 > max_q {
        return Err(format!("Queue is full (max {} tracks).", max_q));
    }
    if is_url
        && !data.config.voice_allow_duplicate_urls
        && queue_contains_url_source(&handler, &query)
    {
        return Err("That URL is already in the queue.".into());
    }

    let verify_immediate = handler.queue().is_empty();
    let added_meta = enqueue_one(
        &mut handler,
        data.http_client.clone(),
        query.clone(),
        is_url,
        cookies_path,
        vol,
        true,
        verify_immediate,
    )
    .await?;
    Ok(EnqueueSummary {
        added: 1,
        first_title: Some(added_meta.title),
    })
}

/// Re-enqueue saved sources without Serenity (queue-loop replay).
pub async fn replay_queue_snapshot(
    manager: &Arc<songbird::Songbird>,
    guild_id: serenity::GuildId,
    music: &Arc<MusicState>,
    http_client: reqwest::Client,
    cookies_path: Option<String>,
    max_queue_tracks: usize,
    voice_allow_duplicate_urls: bool,
) -> Result<(), String> {
    let snapshot = music.queue_loop_snapshot(guild_id.get());
    if snapshot.is_empty() {
        return Ok(());
    }
    let Some(handler_lock) = manager.get(guild_id) else {
        return Err("Voice session ended.".into());
    };
    let mut handler = handler_lock.lock().await;
    let vol = music.volume_for_guild(guild_id.get());
    for (query, is_url) in snapshot {
        if max_queue_tracks > 0 && queued_track_count(&handler) >= max_queue_tracks {
            warn!(
                "Queue loop replay stopped early in guild {} (queue full)",
                guild_id
            );
            break;
        }
        if is_url
            && !voice_allow_duplicate_urls
            && queue_contains_url_source(&handler, &query)
        {
            continue;
        }
        enqueue_one(
            &mut handler,
            http_client.clone(),
            query,
            is_url,
            cookies_path.clone(),
            vol,
            false,
            false,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_queue_url_trims_and_lowercases() {
        assert_eq!(
            normalize_queue_url(" HTTPS://Example.com/X "),
            "https://example.com/x"
        );
    }

    #[test]
    fn parses_flat_playlist_json_line() {
        let line = r#"{"_type":"url","webpage_url":"https://www.youtube.com/watch?v=abc","title":"Test","duration":90.0}"#;
        let row: YtdlJsonLine = serde_json::from_str(line).unwrap();
        assert_eq!(
            row.webpage_url.as_deref(),
            Some("https://www.youtube.com/watch?v=abc")
        );
        assert_eq!(row.title.as_deref(), Some("Test"));
    }

    #[test]
    fn playlist_container_line_has_type() {
        let line = r#"{"_type":"playlist","id":"PLx"}"#;
        let row: YtdlJsonLine = serde_json::from_str(line).unwrap();
        assert_eq!(row.ytdl_type.as_deref(), Some("playlist"));
    }
}
