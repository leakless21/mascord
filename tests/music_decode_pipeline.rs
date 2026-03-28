//! End-to-end **decode** check: `yt-dlp` streams bytes into `ffmpeg` (no Songbird, no Discord).
//! Complements [`music_no_discord`] (metadata + `Track` build) and [`queue_ops`] unit tests.
//!
//! ```text
//! cargo test -p mascord --test music_decode_pipeline -- --ignored --nocapture
//! ```

use std::process::Command;

const ARCHIVE_MP3: &str = "https://archive.org/download/testmp3testfile/mpthreetest.mp3";

#[test]
#[ignore = "network: full yt-dlp → ffmpeg decode; run with --ignored"]
fn ytdlp_pipes_to_ffmpeg_decode_null() {
    let script = format!(
        "set -euo pipefail; yt-dlp -f bestaudio/best -o - --no-playlist {:?} \
         | ffmpeg -hide_banner -loglevel error -i pipe:0 -f null -",
        ARCHIVE_MP3
    );
    let st = Command::new("sh")
        .args(["-c", &script])
        .status()
        .expect("spawn sh -c pipeline");
    assert!(
        st.success(),
        "pipeline failed (need yt-dlp + ffmpeg on PATH and outbound HTTP)"
    );
}
