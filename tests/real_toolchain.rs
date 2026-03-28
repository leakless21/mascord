//! Host + network checks for the music stack (not run in default CI unless you opt in).
//!
//! ```text
//! # Required binaries (always run)
//! cargo test -p mascord --test real_toolchain
//!
//! # Optional: live HTTP metadata (Archive.org; needs outbound network)
//! cargo test -p mascord --test real_toolchain -- --ignored --nocapture
//!
//! # Optional: YouTube (set `YOUTUBE_COOKIES` to an existing cookies.txt, then run the ignored test only)
//! YOUTUBE_COOKIES=/path/to/cookies.txt cargo test -p mascord --test real_toolchain youtube_metadata_with_cookies -- --ignored --nocapture
//!
//! Running **all** ignored tests without `YOUTUBE_COOKIES` still passes: the YouTube test skips with a message.
//! ```

use std::process::Command;

#[test]
fn yt_dlp_binary_available() {
    let out = Command::new("yt-dlp")
        .arg("--version")
        .output()
        .expect("spawn yt-dlp — install yt-dlp and ensure it is on PATH");
    assert!(
        out.status.success(),
        "yt-dlp --version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ver = String::from_utf8_lossy(&out.stdout);
    assert!(!ver.trim().is_empty(), "empty yt-dlp version output");
}

#[test]
fn ffmpeg_binary_available() {
    let out = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .expect("spawn ffmpeg — install ffmpeg and ensure it is on PATH");
    assert!(
        out.status.success(),
        "ffmpeg -version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Live metadata fetch (no YouTube bot wall). Ignored so offline / sandbox CI stays green.
#[test]
#[ignore = "network: run with `cargo test --test real_toolchain -- --ignored`"]
fn yt_dlp_json_metadata_archive_org() {
    let out = Command::new("yt-dlp")
        .args(["-j", "--no-warnings", "--flat-playlist", "--no-playlist"])
        .arg("https://archive.org/download/testmp3testfile/mpthreetest.mp3")
        .output()
        .expect("spawn yt-dlp");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().next().expect("at least one JSON line");
    assert!(
        line.contains("\"title\"") || line.contains("\"id\""),
        "unexpected JSON: {}",
        &line[..line.len().min(200)]
    );
}

/// Proves YouTube works when cookies are supplied (same as production).
/// Skips (passes) if `YOUTUBE_COOKIES` is unset so `cargo test -- --ignored` stays usable.
#[test]
#[ignore = "optional YouTube probe; run with `YOUTUBE_COOKIES` set: cargo test --test real_toolchain youtube_metadata_with_cookies -- --ignored --nocapture`"]
fn youtube_metadata_with_cookies() {
    let path = match std::env::var("YOUTUBE_COOKIES") {
        Ok(p) if std::path::Path::new(&p).exists() => p,
        Ok(p) => panic!("YOUTUBE_COOKIES set but file missing: {p}"),
        Err(_) => {
            eprintln!("SKIP youtube_metadata_with_cookies: YOUTUBE_COOKIES not set");
            return;
        }
    };
    let out = Command::new("yt-dlp")
        .args(["-j", "--no-warnings", "--no-playlist", "--cookies", &path])
        .arg("https://www.youtube.com/watch?v=jNQXAC9IVRw")
        .output()
        .expect("spawn yt-dlp");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("youtube") || stdout.contains("jNQXAC9IVRw") || stdout.contains("\"id\""),
        "unexpected output: {}",
        &stdout[..stdout.len().min(300)]
    );
}
