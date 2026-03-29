//! Lavalink + mascord wiring (no Discord gateway).
//!
//! Exercises the same `LavalinkClient` setup as bot startup (`lavalink_client_for_integration` /
//! `create_lavalink_client`): WebSocket to the node, REST `load_tracks`, plugin manifest from
//! `/v4/info`, LavaSearch (`/v4/loadsearch`), and LavaLyrics (`/v4/lyrics`).
//!
//! ```text
//! # Local Lavalink (e.g. docker/lavalink) on 127.0.0.1:2333 with default password
//! cargo test -p mascord --test lavalink_integration -- --ignored --nocapture
//! ```

use lavalink_rs::model::GuildId as LavaGuildId;
use lavalink_rs::model::track::{TrackLoadData, TrackLoadType};

fn lavalink_http_base(host: &str) -> String {
    if host.starts_with("http://") || host.starts_with("https://") {
        host.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", host.trim_end_matches('/'))
    }
}

#[tokio::test]
#[ignore = "requires local Lavalink; see docker/lavalink/"]
async fn mascord_lavalink_client_connects_and_loads_ytsearch() {
    let host = std::env::var("LAVALINK_HOST").unwrap_or_else(|_| "127.0.0.1:2333".to_string());
    let password =
        std::env::var("LAVALINK_PASSWORD").unwrap_or_else(|_| "youshallnotpass".to_string());
    let app_id: u64 = std::env::var("LAVALINK_TEST_APPLICATION_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let client =
        mascord::commands::music::lavalink::lavalink_client_for_integration(host.clone(), password.clone(), app_id)
            .await;

    // Allow the node WebSocket to mark the node as running (MainFallback node pick).
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let ver = client
        .request_version(LavaGuildId(1))
        .await
        .expect("Lavalink REST version (same stack as /play load_tracks)");
    assert!(
        !ver.is_empty(),
        "expected non-empty version string from server"
    );

    let info = client
        .request_info(LavaGuildId(1))
        .await
        .expect("/v4/info");
    let names: Vec<&str> = info.plugins.iter().map(|p| p.name.as_str()).collect();
    for required in [
        "youtube-plugin",
        "lavasrc-plugin",
        "lavasearch-plugin",
        "lavalyrics-plugin",
        "java-lavalyrics",
        "sponsorblock-plugin",
    ] {
        assert!(
            names.contains(&required),
            "missing plugin {required}, got {names:?}"
        );
    }

    let loaded = client
        .load_tracks(
            LavaGuildId(1),
            "ytsearch:never gonna give you up official",
        )
        .await
        .expect("load_tracks");

    assert_eq!(
        loaded.load_type,
        TrackLoadType::Search,
        "expected ytsearch to return Search, got {:?}",
        loaded
    );
    match loaded.data {
        Some(TrackLoadData::Search(tracks)) => {
            assert!(
                !tracks.is_empty(),
                "expected at least one search result"
            );
        }
        other => panic!("expected Search data, got {:?}", other),
    }

    // LavaSearch: YouTube Music prefix (requires lavasrc `youtube: true` + lavasearch-plugin).
    let http = reqwest::Client::new();
    let base = lavalink_http_base(&host);
    let search_url = reqwest::Url::parse_with_params(
        &format!("{}/v4/loadsearch", base),
        &[
            ("query", "ytmsearch:never gonna give you up"),
            ("types", "track"),
        ],
    )
    .expect("loadsearch url");
    let search_resp = http
        .get(search_url)
        .header("Authorization", &password)
        .send()
        .await
        .expect("loadsearch request");
    assert!(
        search_resp.status().is_success(),
        "loadsearch HTTP {}",
        search_resp.status()
    );
    let search_json: serde_json::Value = search_resp.json().await.expect("loadsearch json");
    let tracks = search_json["tracks"].as_array().expect("loadsearch tracks array");
    assert!(
        !tracks.is_empty(),
        "expected LavaSearch ytmsearch to return tracks: {search_json}"
    );

    // LavaLyrics (+ Java Timed Lyrics bridge registers as `java-lavalyrics` in /v4/info).
    let encoded = tracks[0]["encoded"].as_str().expect("encoded track");
    let lyrics_url = reqwest::Url::parse_with_params(
        &format!("{}/v4/lyrics", base),
        &[("track", encoded), ("skipTrackSource", "false")],
    )
    .expect("lyrics url");
    let lyrics_resp = http
        .get(lyrics_url)
        .header("Authorization", &password)
        .send()
        .await
        .expect("lyrics request");
    assert!(
        lyrics_resp.status().is_success(),
        "lyrics HTTP {}",
        lyrics_resp.status()
    );
    let lyrics_json: serde_json::Value = lyrics_resp.json().await.expect("lyrics json");
    let lines = lyrics_json["lines"].as_array().expect("lyrics lines");
    assert!(
        !lines.is_empty(),
        "expected timestamped/plain lyrics lines from LavaLyrics pipeline: {lyrics_json}"
    );
}
