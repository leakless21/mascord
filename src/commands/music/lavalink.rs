//! Lavalink-backed playback (Songbird `join_gateway` + lavalink-rs `PlayerContext`).

use crate::commands::music::playback::{EnqueueOpts, EnqueueSummary, MAX_PLAYLIST_ENTRIES};
use crate::commands::music::state::MusicState;
use crate::Data;
use lavalink_rs::client::LavalinkClient;
use lavalink_rs::model::client::NodeDistributionStrategy;
use lavalink_rs::model::events::Events;
use lavalink_rs::model::track::{Track, TrackLoadData, TrackLoadType};
use lavalink_rs::model::GuildId as LavaGuildId;
use lavalink_rs::node::NodeBuilder;
use lavalink_rs::player_context::TrackInQueue;
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tracing::{info, warn};

/// Same node wiring as startup; exposed for integration tests (`tests/lavalink_integration.rs`).
pub async fn lavalink_client_for_integration(
    host: impl Into<String>,
    password: impl Into<String>,
    application_id: u64,
) -> Arc<LavalinkClient> {
    let node = NodeBuilder {
        hostname: host.into(),
        password: password.into(),
        user_id: lavalink_rs::model::UserId(application_id),
        ..Default::default()
    };
    Arc::new(
        LavalinkClient::new(
            Events::default(),
            vec![node],
            NodeDistributionStrategy::MainFallback,
        )
        .await,
    )
}

pub async fn create_lavalink_client(
    config: &crate::config::Config,
) -> Option<Arc<LavalinkClient>> {
    if !config.lavalink_enabled {
        return None;
    }
    Some(
        lavalink_client_for_integration(
            config.lavalink_host.clone(),
            config.lavalink_password.clone(),
            config.application_id,
        )
        .await,
    )
}

fn normalize_uri(s: &str) -> String {
    s.trim().to_lowercase()
}

/// True when the query already uses a Lavalink/LavaSrc load prefix (`ytsearch:`, `ytmsearch:`, `spsearch:`, …).
fn has_explicit_lavalink_search_prefix(query: &str) -> bool {
    let q = query.trim();
    let Some(idx) = q.find(':') else {
        return false;
    };
    let pref = q[..idx].to_ascii_lowercase();
    matches!(
        pref.as_str(),
        "ytsearch"
            | "ytmsearch"
            | "ymsearch"
            | "spsearch"
            | "dzsearch"
            | "amsearch"
            | "scsearch"
            | "dzisrc"
            | "vksearch"
            | "ytdlpsearch"
            | "jssearch"
    ) || pref.ends_with("search")
}

fn identifier_for_query(query: &str, is_url: bool, default_prefix: &str) -> String {
    if is_url {
        return query.trim().to_string();
    }
    let q = query.trim();
    let lower = q.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return q.to_string();
    }
    if has_explicit_lavalink_search_prefix(q) {
        return q.to_string();
    }
    let p = default_prefix.trim().trim_end_matches(':');
    format!("{}:{}", p, q)
}

/// Query text for LavaSearch `/v4/loadsearch`.
/// LavaSearch expects plain search terms, not Lavalink load prefixes.
fn lavasearch_query_for(query: &str) -> String {
    let q = query.trim();
    let Some(idx) = q.find(':') else {
        return q.to_string();
    };
    if has_explicit_lavalink_search_prefix(q) {
        return q[idx + 1..].trim().to_string();
    }
    q.to_string()
}

async fn apply_sponsorblock_categories_if_configured(data: &Data, guild_id: serenity::GuildId) {
    let Some(ref cats) = data.config.lavalink_sponsorblock_categories else {
        return;
    };
    if cats.is_empty() {
        return;
    }
    let Some(ll) = &data.lavalink else {
        return;
    };
    let Some(node) = ll.nodes.first() else {
        return;
    };
    let sid = node.session_id.load_full();
    if sid.is_empty() {
        tracing::debug!("SponsorBlock: Lavalink session id not ready yet");
        return;
    }
    if let Err(e) = crate::services::lavalink_rest::put_sponsorblock_categories(
        &data.http_client,
        &data.config.lavalink_host,
        &data.config.lavalink_password,
        &sid,
        guild_id.get(),
        cats,
    )
    .await
    {
        tracing::warn!("SponsorBlock category update failed: {}", e);
    }
}

/// Join voice with Songbird gateway only (no local decode driver) and ensure a Lavalink player exists.
pub async fn join_lavalink_voice(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    _music: &Arc<MusicState>,
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

    let ll = data
        .lavalink
        .as_ref()
        .ok_or_else(|| "Lavalink is not configured.".to_string())?;

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized.".to_string())?
        .clone();

    if manager.get(guild_id).is_none() {
        info!(
            "Joining voice (Lavalink gateway) channel {} in guild {}",
            channel_id, guild_id
        );
        let (connection_info, _call) = manager
            .join_gateway(guild_id, channel_id)
            .await
            .map_err(|e| format!("Failed to join voice gateway: {}", e))?;
        ll.create_player_context(LavaGuildId(guild_id.get()), connection_info)
            .await
            .map_err(|e| format!("Lavalink create player: {}", e))?;
        apply_sponsorblock_categories_if_configured(data, guild_id).await;
        return Ok(channel_id);
    }

    if ll.get_player_context(LavaGuildId(guild_id.get())).is_none() {
        let handler_lock = manager
            .get(guild_id)
            .ok_or_else(|| "Voice handler missing.".to_string())?;
        let handler = handler_lock.lock().await;
        let connection_info = handler
            .current_connection()
            .ok_or_else(|| "Voice connection not ready yet — try again.".to_string())?
            .clone();
        drop(handler);
        ll.create_player_context(LavaGuildId(guild_id.get()), connection_info)
            .await
            .map_err(|e| format!("Lavalink create player: {}", e))?;
        apply_sponsorblock_categories_if_configured(data, guild_id).await;
    }

    Ok(channel_id)
}

fn track_data_list(loaded: Track) -> Result<Vec<lavalink_rs::model::track::TrackData>, String> {
    match (loaded.load_type, loaded.data) {
        (_, Some(TrackLoadData::Error(e))) => Err(e.message),
        (TrackLoadType::Track, Some(TrackLoadData::Track(t))) => Ok(vec![t]),
        (TrackLoadType::Search, Some(TrackLoadData::Search(mut tracks))) => {
            if tracks.is_empty() {
                Err("No search results.".into())
            } else {
                Ok(vec![tracks.remove(0)])
            }
        }
        (TrackLoadType::Playlist, Some(TrackLoadData::Playlist(pl))) => {
            if pl.tracks.is_empty() {
                Err("Playlist is empty.".into())
            } else {
                Ok(pl.tracks)
            }
        }
        (TrackLoadType::Empty, _) => Err("No matches for that query.".into()),
        _ => Err("Could not load audio (unexpected Lavalink response).".into()),
    }
}

async fn queue_contains_uri(
    ll: &Arc<LavalinkClient>,
    guild_id: serenity::GuildId,
    uri: &str,
) -> bool {
    let Some(p) = ll.get_player_context(LavaGuildId(guild_id.get())) else {
        return false;
    };
    let needle = normalize_uri(uri);
    if let Ok(pl) = p.get_player().await {
        if let Some(t) = pl.track {
            if let Some(u) = t.info.uri {
                if normalize_uri(&u) == needle {
                    return true;
                }
            }
        }
    }
    let q = p.get_queue();
    if let Ok(dq) = q.get_queue().await {
        for x in dq {
            if let Some(u) = x.track.info.uri {
                if normalize_uri(&u) == needle {
                    return true;
                }
            }
        }
    }
    false
}

async fn apply_session_volume(
    player: &lavalink_rs::player_context::PlayerContext,
    guild_id: u64,
    music: &Arc<MusicState>,
) -> Result<(), String> {
    let vol_u16 = ((music.volume_for_guild(guild_id) * 100.0).round() as u32).min(1000) as u16;
    player.set_volume(vol_u16).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn enqueue_lavalink_playback(
    serenity_ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    requester: serenity::UserId,
    query: String,
    music: &Arc<MusicState>,
    opts: EnqueueOpts,
) -> Result<EnqueueSummary, String> {
    let ll = data
        .lavalink
        .as_ref()
        .ok_or_else(|| "Lavalink is not configured.".to_string())?;

    let manager = songbird::get(serenity_ctx)
        .await
        .ok_or_else(|| "Songbird not initialized.".to_string())?
        .clone();

    if manager.get(guild_id).is_none() {
        if opts.skip_voice_check {
            return Err("Bot is not in a voice channel.".into());
        }
        join_lavalink_voice(serenity_ctx, data, guild_id, requester, music).await?;
    }

    let player = ll
        .get_player_context(LavaGuildId(guild_id.get()))
        .ok_or_else(|| "Lavalink player missing — try `/join` or play again.".to_string())?;

    apply_session_volume(&player, guild_id.get(), music).await?;

    let is_url = query.starts_with("http://") || query.starts_with("https://");
    let max_q = data.config.max_queue_tracks;
    let qref = player.get_queue();

    let mut session_queued = qref.get_count().await.map_err(|e| e.to_string())?;
    if player
        .get_player()
        .await
        .map_err(|e| e.to_string())?
        .track
        .is_some()
    {
        session_queued = session_queued.saturating_add(1);
    }

    if is_url && opts.expand_playlist {
        let identifier = query.trim().to_string();
        let loaded = ll
            .load_tracks(LavaGuildId(guild_id.get()), &identifier)
            .await
            .map_err(|e| e.to_string())?;
        let mut tracks = track_data_list(loaded)?;
        if tracks.len() > MAX_PLAYLIST_ENTRIES {
            warn!(
                "Playlist truncated: {} → {} tracks",
                tracks.len(),
                MAX_PLAYLIST_ENTRIES
            );
            tracks.truncate(MAX_PLAYLIST_ENTRIES);
        }
        if !data.config.voice_allow_duplicate_urls {
            let mut kept = Vec::new();
            for td in tracks {
                let dup = match &td.info.uri {
                    Some(u) => queue_contains_uri(ll, guild_id, u).await,
                    None => false,
                };
                if !dup {
                    kept.push(td);
                }
            }
            tracks = kept;
        }
        if tracks.is_empty() {
            return Err("All playlist entries were already in the queue.".into());
        }
        if max_q > 0 {
            let slots = max_q.saturating_sub(session_queued);
            if slots == 0 {
                return Err(format!("Queue is full (max {} tracks).", max_q));
            }
            if tracks.len() > slots {
                tracks.truncate(slots);
            }
        }

        let was_idle = qref.get_count().await.map_err(|e| e.to_string())? == 0
            && player
                .get_player()
                .await
                .map_err(|e| e.to_string())?
                .track
                .is_none();

        let mut first_title: Option<String> = None;
        let n = tracks.len();
        for td in tracks {
            if first_title.is_none() {
                first_title = Some(td.info.title.clone());
            }
            qref.push_to_back(TrackInQueue::from(td))
                .map_err(|e| e.to_string())?;
        }
        if was_idle {
            player.skip().map_err(|e| e.to_string())?;
        }
        info!("Queued {} Lavalink tracks from playlist in guild {}", n, guild_id);
        return Ok(EnqueueSummary {
            added: n,
            first_title: first_title.or_else(|| Some(query.clone())),
        });
    }

    if max_q > 0 && session_queued + 1 > max_q {
        return Err(format!("Queue is full (max {} tracks).", max_q));
    }

    let search_identifier = identifier_for_query(
        &query,
        is_url,
        &data.config.lavalink_search_prefix,
    );

    let td = if !is_url && !opts.expand_playlist && data.config.lavalink_use_lavasearch {
        let search_query = lavasearch_query_for(&query);
        let Some(enc) = crate::services::lavalink_rest::loadsearch_first_encoded(
            &data.http_client,
            &data.config.lavalink_host,
            &data.config.lavalink_password,
            &search_query,
        )
        .await?
        else {
            return Err("No search results.".into());
        };
        ll.decode_track(LavaGuildId(guild_id.get()), &enc)
            .await
            .map_err(|e| e.to_string())?
    } else {
        let loaded = ll
            .load_tracks(LavaGuildId(guild_id.get()), &search_identifier)
            .await
            .map_err(|e| e.to_string())?;
        let mut tracks = track_data_list(loaded)?;
        tracks.pop().ok_or_else(|| "No track loaded.".to_string())?
    };
    let title = td.info.title.clone();

    if is_url && !data.config.voice_allow_duplicate_urls {
        if let Some(ref u) = td.info.uri {
            if queue_contains_uri(ll, guild_id, u).await {
                return Err("That URL is already in the queue.".into());
            }
        }
    }

    let was_idle = qref.get_count().await.map_err(|e| e.to_string())? == 0
        && player
            .get_player()
            .await
            .map_err(|e| e.to_string())?
            .track
            .is_none();

    let tq = TrackInQueue::from(td);
    if was_idle {
        player
            .play_now(&tq.track)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        qref.push_to_back(tq).map_err(|e| e.to_string())?;
    }

    Ok(EnqueueSummary {
        added: 1,
        first_title: Some(title),
    })
}
