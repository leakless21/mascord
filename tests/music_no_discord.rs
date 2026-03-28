//! Music stack checks **without** Discord voice / gateway.
//!
//! What this covers (same building blocks as slash `play` / playlist expand):
//! - [`mascord::commands::music::playback::fetch_playlist_entries`] (yt-dlp flat JSON)
//! - Songbird [`YoutubeDl`] + [`songbird::input::Compose::aux_metadata`] (enqueue preflight)
//! - Building a [`songbird::tracks::Track`] from that input (no `Call::enqueue`)
//!
//! What still needs Discord (or Songbird’s internal `test_cfg`, not exposed to dependents):
//! - Joining a voice channel, `Call::enqueue`, pause/skip/shuffle on the live queue, hearing audio.
//!
//! ```text
//! # Archive.org (outbound HTTP; no YouTube cookies)
//! cargo test -p mascord --test music_no_discord -- --ignored --nocapture
//!
//! # Optional: Songbird path against YouTube with cookies (same env as production)
//! YOUTUBE_COOKIES=/path/to/cookies.txt cargo test -p mascord --test music_no_discord songbird_youtube_aux_with_cookies -- --ignored --nocapture
//! ```

use mascord::commands::music::playback::{fetch_playlist_entries, TrackUserData};
use songbird::input::{Compose, YoutubeDl};
use std::sync::Arc;

const ARCHIVE_MP3: &str = "https://archive.org/download/testmp3testfile/mpthreetest.mp3";

#[tokio::test]
#[ignore = "network: cargo test -p mascord --test music_no_discord -- --ignored --nocapture"]
async fn fetch_playlist_entries_resolves_direct_http_url() {
    let entries = fetch_playlist_entries(ARCHIVE_MP3, None)
        .await
        .expect("fetch_playlist_entries");
    assert_eq!(entries.len(), 1, "single direct URL should yield one row");
    let (url, _title, _dur) = &entries[0];
    assert!(
        url.starts_with("http://") || url.starts_with("https://"),
        "expected absolute URL, got {url}"
    );
}

#[tokio::test]
#[ignore = "network"]
async fn songbird_youtube_dl_aux_metadata_archive() {
    let client = reqwest::Client::new();
    let mut src = YoutubeDl::new(client, ARCHIVE_MP3.to_string());
    src = src.user_args(vec!["--no-playlist".to_string()]);
    let meta = src
        .aux_metadata()
        .await
        .expect("aux_metadata should succeed for Archive.org test file");
    let label = meta
        .title
        .clone()
        .or(meta.track.clone())
        .unwrap_or_default();
    assert!(
        !label.is_empty() || meta.duration.is_some(),
        "expected title or duration from metadata: {:?}",
        meta
    );
}

#[tokio::test]
#[ignore = "network"]
async fn build_track_from_youtube_dl_without_call() {
    let client = reqwest::Client::new();
    let mut src = YoutubeDl::new(client, ARCHIVE_MP3.to_string());
    src = src.user_args(vec!["--no-playlist".to_string()]);
    let meta = src.aux_metadata().await.expect("aux_metadata");
    let ud = Arc::new(TrackUserData {
        title: meta
            .title
            .clone()
            .or(meta.track.clone())
            .unwrap_or_default(),
        duration: meta.duration,
        source: ARCHIVE_MP3.to_string(),
        thumbnail: meta.thumbnail.clone(),
    });
    let input: songbird::input::Input = src.into();
    let _track = songbird::tracks::Track::new_with_data(input, ud).volume(1.0);
}

/// Same Songbird + cookie wiring as production `enqueue_one` preflight for YouTube URLs.
#[tokio::test]
#[ignore = "optional YouTube; run with YOUTUBE_COOKIES set"]
async fn songbird_youtube_aux_with_cookies() {
    let path = match std::env::var("YOUTUBE_COOKIES") {
        Ok(p) if std::path::Path::new(&p).exists() => p,
        Ok(p) => panic!("YOUTUBE_COOKIES set but file missing: {p}"),
        Err(_) => {
            eprintln!("SKIP songbird_youtube_aux_with_cookies: YOUTUBE_COOKIES not set");
            return;
        }
    };
    let client = reqwest::Client::new();
    let url = "https://www.youtube.com/watch?v=jNQXAC9IVRw".to_string();
    let mut src = YoutubeDl::new(client, url.clone());
    src = src.user_args(vec![
        "--no-playlist".to_string(),
        "--cookies".to_string(),
        path,
    ]);
    let meta = src.aux_metadata().await.expect("YouTube aux_metadata");
    assert!(
        meta.title.is_some() || meta.track.is_some(),
        "expected a title-like field: {:?}",
        meta
    );
}
