# Mascord: The Agentic Discord Assistant

**Mascord** is a high-performance, modular Discord bot written in Rust. It combines any **OpenAI-compatible LLM** (like `llama.cpp`, LocalAI, vLLM, or OpenAI itself), Retrieval-Augmented Generation (RAG), and a native music player with an advanced **Agentic** core powered by built-in native tools.

---

## ✨ Key Features

- 🧠 **Three-Tier Persistent Memory**: Advanced context management featuring:
  - **Short-Term**: Verbatim recent conversation history (last 50 messages).
  - **Working Memory**: Condensing older interactions into persistent summaries via autonomous background jobs or manual triggers.
  - **Long-Term**: On-demand retrieval and LLM-powered summarization of historical messages (RAG).
- 🎵 **Interactive Music Player**: `yt-dlp` + Songbird with:
  - **Queue & controls**: Titles, durations, estimated total length, playlist import (optional), shuffle/clear/move/remove, volume, pause/resume, now playing, **track** and **queue** loop modes.
  - **Agent tool `music`**: Same playback/queue stack as slash commands when you mention the bot or reply (`action` mirrors `/play`, `/skip`, `/volume`, etc.).
  - **Cookies**: Optional `YOUTUBE_COOKIES` for age-restricted or bot-check streams.
- 🤖 **Agentic Core**: An autonomous agent trained to use internal and external tools (including native web search and URL fetch) to solve complex, multi-step requests.
- ⚙️ **Configurable Settings**: Per-guild configuration for context limits, retention policies, and manual working-memory refreshes.
- ⏰ **Reminder Scheduler**: Durable natural-language reminders (`in 2 days`, `3 hours`, `at 22:15`) with list/cancel support and background delivery.

---

## 🚀 Setup & Installation

**Setup and operations:** **[docs/setup.md](docs/setup.md)**

At a glance:

1. **Dependencies:** Rust, `ffmpeg`, `yt-dlp`, `cmake`, `pkg-config`, plus an OpenAI-compatible LLM API.
2. **Configure:** `cp .env.example .env` and set `DISCORD_TOKEN`, `APPLICATION_ID`, `LLAMA_URL`, `LLAMA_MODEL`, `DATABASE_URL`. Enable **Message Content Intent** (Developer Portal → Bot → Privileged Gateway Intents). Many more options are documented in [`.env.example`](.env.example) and [docs/setup.md](docs/setup.md).
3. **Database:** SQLite is initialized on first run (`mkdir -p data` if the parent path does not exist).
4. **Build / run:** `cargo build --release`, then `./bot.sh` or `cargo run --release`.
5. **Slash commands:** Do not leave `REGISTER_COMMANDS=true` for every restart — see [docs/setup.md](docs/setup.md).

**Doc index:** [docs/README.md](docs/README.md)

### Native web tools

Mascord now ships with native tools for live web research:

- `web_search`: Queries your configured SearXNG instance (`SEARXNG_URL`)
- `fetch_url`: Fetches and extracts readable text from HTTP/HTTPS pages (readability-optimized)
- `fetch_url` with `render_javascript=true`: Uses Jina AI Reader for JS-heavy pages (no API key)
- `fetch_url` with `render_mode=auto`: Uses local fetch first, then auto-falls back to Jina Reader when content quality is poor

These run in-process; no separate MCP server is involved.

---

## Monitoring Your Bot

With **systemd**, use **`systemctl`** and **`journalctl`** first (state and logs). Optional **`HEALTH_PORT`** HTTP probes are for extra checks (e.g. Uptime Kuma on localhost). Details: **[docs/setup.md#monitoring-systemd](docs/setup.md#monitoring-systemd)**.

```bash
sudo systemctl status mascord
sudo journalctl -u mascord -f --no-pager
```

### Stop the Bot

**systemd:** `sudo systemctl stop mascord`  
**Manual run:** `pkill -f "target/release/mascord"`  
**Discord (owner):** `/shutdown`

### Troubleshooting

See **[docs/setup.md#troubleshooting](docs/setup.md#troubleshooting)**. Quick checks: `mkdir -p data/`, valid `.env`, `curl` to your LLM `/models`, Discord permissions and Message Content Intent.

---

## 🎮 Usage Guide

### 🧬 The Memory System
Mascord doesn't just "see" the last message. It manages context in three layers:
1. **Passive Observation**: The bot reads all messages (even without mentions) to maintain a live history.
2. **Conversation Context**: When you **mention the bot** or **reply** to it, the agent automatically pulls the last ~50 messages into the LLM's prompt.
3. **Working Memory**: For very long conversations, use `/settings context summarize`. This condenses the history into a "Working Memory" snippet that the bot always sees.
4. **Historical Search**: Use `/search` or tell the bot to "search for X" to trigger the RAG engine over months of historical logs.

### 🎵 Music Player Tips
- **Buttons**: `/queue` uses **buttons** (not reactions) for pause/resume, skip, stop, and page through upcoming tracks.
- **Playlists**: Set `playlist:true` on `/play` with a playlist URL to queue up to 50 entries.
- **Cookies**: If you encounter `403 Forbidden` or age-gate errors from YouTube, export browser cookies to a `cookies.txt` and set `YOUTUBE_COOKIES` in `.env`.

### 🤖 Multi-Step Tasks (Agent)
**Mention the bot** or **reply** to its messages for requests that require multiple actions (including tools).
*Example: "@Mascord Search for the last time we talked about the API design, summarize it, and then play some lofi music."*

## Future capabilities (roadmap)

- [ ] **Multimodal Search**: Index image attachments and embeds in RAG using CLIP/SigLIP models.
- [ ] **Vision Support**: Enable LLM to analyze images and screenshots shared in Discord.
- [ ] **Enhanced Audio**: Optional Spotify / local library integrations (beyond `yt-dlp` URLs).

---

## 📋 Available Commands

| Command | Description |
|---------|-------------|
| `/about` | Show onboarding info, key commands, and deployment safety notes. |
| *(no slash)* | **Mention** the bot or **reply** to it to chat with the agent using current context memory. |
| `/search` | Manually search through the RAG database. |
| `/memory` | Manage global user memory (`enable`, `disable`, `show`, `remember`, `forget`, `delete_data`). |
| `/reminder` | Set reminders directly (`when` + `message`) or manage via `action` (`list`, `cancel`, `help`). |
| `/join` | Join your current voice channel. |
| `/play` | Queue audio (search, URL, or playlist with `playlist:true`). |
| `/skip` | Skip the current track. |
| `/leave` | Leave voice and end the session. |
| `/queue` | Queue view with titles, duration, and controls. |
| `/nowplaying` | Current track details. |
| `/pause` / `/resume` | Pause or resume. |
| `/volume` | 0–200% session volume. |
| `/loop` | Off, repeat current track, or repeat queue snapshot. |
| `/clear` / `/shuffle` | Clear upcoming or shuffle upcoming. |
| `/remove` / `/move_track` | Remove or reorder by position. |
| `/settings` | Manage context/memory/system prompt/timeouts per guild. |
| `/shutdown` | Exit process (owner only; hidden from help). |
| `/restart` | Graceful shard shutdown for supervisor restart (owner only). |

---

## 📖 Documentation

- **[docs/README.md](docs/README.md)** — Index
- **[docs/setup.md](docs/setup.md)** — Install, env, registration, deploy, **monitoring**, troubleshooting
- **[docs/commands.md](docs/commands.md)** — Slash commands
- **[docs/internals.md](docs/internals.md)** — Architecture and module map

---

## ✅ Verify build and host toolchain

```bash
cargo test                           # unit tests (includes music queue index helpers)
cargo test --test real_toolchain     # yt-dlp + ffmpeg on PATH
cargo test --test real_toolchain -- --ignored   # + live metadata (Archive.org; needs network)
cargo test --test music_no_discord -- --ignored # Songbird input + metadata (no Discord)
cargo test --test music_decode_pipeline -- --ignored # yt-dlp | ffmpeg full decode (no Discord)
```

**Queue UX, pause/skip, and embeds** still need a live Discord voice session: Songbird’s `Config::test_cfg` / fake `Driver` output is only built when compiling Songbird itself, not when mascord depends on it. Manually verify on a staging server using the Voice / Music sections in [commands](docs/commands.md) (`/play`, `/queue` with buttons, `/shuffle`, `/move`, etc.).

With a valid `cookies.txt`, you can also prove YouTube extraction:

```bash
YOUTUBE_COOKIES=/path/to/cookies.txt \
  cargo test -p mascord --test real_toolchain youtube_metadata_with_cookies -- --ignored --nocapture
```

YouTube often returns “sign in to confirm you’re not a bot” **without** cookies; that is expected on many networks.

## 🤝 Contribution

Mascord follows a modular architecture. Feel free to contribute by adding new tools to `src/tools/` or extending the Agentic capabilities with additional native integrations.

---

*Built with ❤️ using Serenity, Poise, and Songbird.*
