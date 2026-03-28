# Commands

Discord’s slash UI lists options; this is a concise reference.

## Summary

| Area | Commands |
|------|----------|
| Chat / agent | `/about`, `/chat` |
| Search / memory | `/search`, `/memory`, `/settings memory` (mods) |
| Reminders | `/reminder` (`when` + `message`, or `action`: list / cancel / help) |
| Music | `/join`, `/play`, `/skip`, `/leave`, `/queue`, `/nowplaying`, `/pause`, `/resume`, `/volume`, `/loop`, `/clear`, `/shuffle`, `/remove`, `/move_track` |
| Settings | `/settings` — context, memory, `system_prompt`, `agent_timeout`, `voice_timeout` |
| Owner | `/shutdown`, `/restart` (hidden; `OWNER_ID` only) |

## Chat

**`/chat`** — Sends message + recent context to the LLM. Can use built-in tools (RAG, web if configured, `play_music` in a server voice channel). Direct play intents (`play ...`, `queue ...`, `put on ...`, `add to queue ...`) are fast-routed to the native music pipeline before LLM reasoning, so obvious music requests do not depend on model tool-calling reliability. Tools requiring confirmation use `AGENT_CONFIRM_TIMEOUT_SECS` / `/settings agent_timeout`.

**`/search`** — Embedding search over stored history (hybrid + filters). Moderators use **`/settings memory`** for per-channel tracking, scope, purge.

## User memory

**`/memory`** — Global opt-in profile: `enable`, `disable`, `show`, `remember`, `forget`, `delete_data`.

## Reminders

**`/reminder`** — Natural language: `when:in 2 days, 30 minutes`, `3 hours`, `at 22:15`, absolute UTC datetimes. `action:list`, `action:cancel reminder_id:<id>`, `action:help`.

## Music

Requires **Connect** + **Speak**. **`/play`** auto-joins your voice channel if needed. Sources via `yt-dlp` (search, URL, optional playlist with `playlist:true` on URLs — capped). **`YOUTUBE_COOKIES`** helps age-restricted streams.

**`/queue`** — Titles, duration, **buttons** (not reactions) for controls. **`/loop`** — `off` / `track` / `queue` (queue replays a snapshot).

## Settings

- **`/settings context`** — get, set (`limit`, `retention`), `summarize` (working memory).
- **`/settings system_prompt`** — view/set/reset.
- **`/settings agent_timeout`** / **`voice_timeout`** — guild overrides.

## Permissions

Send/read messages, embed links, connect/speak for voice; `Manage Server` for some memory settings.

## Common errors

| Message | Likely fix |
|---------|------------|
| Command not found | Register commands once, then `REGISTER_COMMANDS=false` |
| Not in voice | Join VC; `/play` can auto-join |
| LLM error | Check `LLAMA_URL`, model name, server up |
| `play_music` returns an error from `/chat` | Be in a **server** (not DMs), in a **voice channel**; rebuild/restart the bot after updating; or use **`play …`** / **`play me …`** at the start of the message (fast route) or **`/play`** |
| DB error | Restart; worst case remove `data/mascord.db` (data loss) |

**Tips:** Reply to the bot to continue a thread. Say “no memory” for a one-off without user memory. Combine multi-step work in one `/chat`.
