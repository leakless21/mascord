# Internals

Architecture, requirements summary, module map, and project status.

## Architecture

Mascord is a Rust Discord bot (Poise/Serenity): **gateway** → commands/handlers → **services** (LLM, RAG, voice, tools, reminders, cache). SQLite holds messages, summaries, settings, reminders, user memory. **OpenAI-compatible** LLM + optional separate embedding endpoint. **Voice:** Songbird + `yt-dlp` + ffmpeg. **Agent:** `ToolRegistry` + built-in tools only (no MCP runtime).

**Three-tier memory:** short-term (recent messages + optional time filter), working (rolling summaries), long-term (RAG + embeddings + indexer). Per-guild overrides via `/settings` in SQLite.

```mermaid
graph LR
  User([User]) <--> Discord[Discord API]
  Discord <--> FW[Framework]
  FW --> Chat[/chat]
  Chat --> Agent[AgentLoop]
  FW --> RAG[RAG]
  FW --> Voice[Voice]
  FW --> Rem[Reminders]
  RAG --> DB[(SQLite)]
  Rem --> DB
  Agent --> LLM[OpenAI-compatible API]
```

## Requirements (summary)

- Slash commands; reply + mention chat; embeds; markdown degraded for Discord.
- LLM + three-tier memory + opt-in global user memory + RAG with hybrid search and channel controls.
- Music: `yt-dlp`, queue, cookies optional, `play_music` tool in guild, plus `/chat` direct play-intent fast route for reliability.
- Reminders: NL schedules, SQLite, dispatch throttled.
- Agent: built-in tools, confirmation for risky tools, iteration limits.
- Owner-only admin; secrets via env; user data deletion path (`/memory delete_data`).
- Config: `.env` + `.env.example`; small footprint; macOS + Linux.

## Module map

| Area | Main paths |
|------|------------|
| Bot / lifecycle | `main.rs`, `config.rs`, `reply.rs`, `mention.rs`, `discord_text.rs` |
| LLM / agent | `llm/client.rs`, `llm/agent.rs` |
| RAG | `rag/mod.rs`, `db/mod.rs` |
| Tools | `tools/` |
| Voice | `commands/music/`, `voice/` |
| Context / summaries | `context.rs`, `summarize.rs`, `cache.rs` |
| Reminders | `reminders.rs`, `services/reminder.rs` |
| User memory | `services/user_memory.rs` |
| System prompt / time | `system_prompt.rs` |

**Prompts:** Main behavior lives in `SYSTEM_PROMPT` (guild override or `config.rs` default). A second system line injects **time only** (`system_prompt.rs`)—no duplicate tool rules; that matches common agent patterns (instructions vs. facts).

## Status and gaps

- **Memory/roadmap work (2026):** Vector search, rolling summarization, music cookies, and agent safety guards are implemented; optional `sqlite-vec` remains a future acceleration path.
- **Open:** At-rest DB encryption not documented (use OS-level encryption if needed). Some commands still touch DB directly instead of a thin service layer (testing/maintainability).
- **Multi-instance:** Use shared DB + `JOB_LEASES_ENABLED`; single writer for background jobs.

## Security note (2026-01-30)

A Brave Search API key was once committed in `mcp_servers.toml`; history was scrubbed and the file is gitignored. **Rotate any exposed keys**; do not commit secrets. Mascord does not ship an MCP client today; treat API keys as env-only.
