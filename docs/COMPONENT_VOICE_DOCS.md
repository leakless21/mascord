# Component: Voice Documentation

## Area of Responsibility
Voice channel management and YouTube audio streaming.

## Key Classes / Modules
- `src/commands/music.rs`: Slash commands for voice interaction.
- `src/voice/mod.rs`: Module setup.

## Interfaces
- **External**: Discord Voice Gateway, YouTube (via `yt-dlp`).
- **Internal**: `Songbird` manager.

## Implementation Details
Uses `songbird` with `builtin-queue` and `yt-dlp` features enabled. Includes:
- **IdleHandler**: Auto-disconnect after a configurable idle timeout (default: 5 minutes).
- **Idle Timeout Override**: Per-guild idle timeout can be configured via `/settings voice_timeout` and stored in SQLite.
- **CleanupService**: Periodic deletion of old `yt-dlp` cache files.
- **Cookie Support**: Passing cookies via `YTDL_ARGS` env var; warns and skips if cookie file path is missing.

## Platform Notes
- Voice playback requires `yt-dlp` and `ffmpeg` available on `PATH`.
- On macOS, install dependencies with Homebrew (e.g., `brew install yt-dlp ffmpeg`).
