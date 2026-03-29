//! Lavalink v4 HTTP helpers for plugin endpoints not wrapped by lavalink-rs (LavaLyrics, SponsorBlock).

use reqwest::header::AUTHORIZATION;
use serde_json::Value;

fn v4_base(host: &str) -> String {
    let h = host
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    format!("http://{}/v4", h.trim_end_matches('/'))
}

/// `GET /v4/lyrics?track=…` — LavaLyrics (+ Java Timed Lyrics bridge on the server).
pub async fn fetch_lyrics_for_encoded_track(
    http: &reqwest::Client,
    host: &str,
    password: &str,
    encoded: &str,
) -> Result<Option<Value>, String> {
    let base = v4_base(host);
    let url = reqwest::Url::parse_with_params(
        &format!("{}/lyrics", base),
        &[
            ("track", encoded),
            ("skipTrackSource", "false"),
        ],
    )
    .map_err(|e| e.to_string())?;
    let resp = http
        .get(url)
        .header(AUTHORIZATION, password)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 204 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("lyrics HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string()).map(Some)
}

/// `PUT /v4/sessions/{sessionId}/players/{guildId}/sponsorblock/categories`
pub async fn put_sponsorblock_categories(
    http: &reqwest::Client,
    host: &str,
    password: &str,
    session_id: &str,
    guild_id: u64,
    categories: &[String],
) -> Result<(), String> {
    let url = format!(
        "{}/sessions/{}/players/{}/sponsorblock/categories",
        v4_base(host),
        session_id,
        guild_id
    );
    let resp = http
        .put(&url)
        .header(AUTHORIZATION, password)
        .json(categories)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("sponsorblock PUT HTTP {}", resp.status()));
    }
    Ok(())
}

/// `GET /v4/loadsearch?query=…&types=track` (LavaSearch plugin). Returns the first hit’s `encoded` payload.
pub async fn loadsearch_first_encoded(
    http: &reqwest::Client,
    host: &str,
    password: &str,
    search_query: &str,
) -> Result<Option<String>, String> {
    let url = reqwest::Url::parse_with_params(
        &format!("{}/loadsearch", v4_base(host)),
        &[("query", search_query), ("types", "track")],
    )
    .map_err(|e| e.to_string())?;
    let resp = http
        .get(url)
        .header(AUTHORIZATION, password)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 204 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("loadsearch HTTP {}", resp.status()));
    }
    let j: Value = resp.json().await.map_err(|e| e.to_string())?;
    let enc = j
        .get("tracks")
        .and_then(|t| t.as_array())
        .and_then(|a| a.first())
        .and_then(|tr| tr.get("encoded"))
        .and_then(|e| e.as_str())
        .map(std::string::ToString::to_string);
    Ok(enc)
}
