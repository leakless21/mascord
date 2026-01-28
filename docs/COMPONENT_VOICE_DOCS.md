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
Uses `songbird` with `builtin-queue` and `yt-dlp` features enabled.
