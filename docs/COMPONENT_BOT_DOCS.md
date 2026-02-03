# Component: Core Bot Documentation

## Area of Responsibility

General bot setup, command registration, and event lifecycle management.

## Key Classes / Modules

- `src/main.rs`: Entry point and Poise framework initialization.
- `src/config.rs`: Configuration handling (env vars, constants).
- `src/commands/mod.rs`: Command registration and grouping.
- `src/summarize.rs`: Background summarization manager (rolling summary with caps, refresh, milestones).

## Configuration & Environment

The bot is configured via environment variables (see `.env.example`). If a variable is missing, the bot uses sensible internal defaults defined in `src/config.rs`.

- `DISCORD_TOKEN`: **Required**. Bot token from Discord Developer Portal.
- `APPLICATION_ID`: **Required**. Discord application ID.
- `OWNER_ID`: (Optional) ID of the bot owner for admin-restricted commands.
- `LLAMA_URL`: (Default: `http://localhost:8080`) Base URL for the LLM API.
- `EMBEDDING_URL`: (Default: `LLAMA_URL`) Base URL for the embedding API.
- `SYSTEM_PROMPT`: (Default: Detailed agent prompt) The core instruction for the assistant.
- `YOUTUBE_COOKIES`: (Optional) Path to cookies file for `yt-dlp`.
- `MCP_TOOLS_REQUIRE_CONFIRMATION`: (Default: `true`) Require user confirmation before executing MCP tools via the agent.
- `AGENT_CONFIRM_TIMEOUT_SECS`: (Default: `300`) How long the bot waits for a user to confirm a tool execution.
- `EMBEDDING_INDEXER_ENABLED`: (Default: `true`) Enable background embedding backfill/indexing.
- `EMBEDDING_INDEXER_BATCH_SIZE`: (Default: `25`) Messages embedded per indexer tick.
- `EMBEDDING_INDEXER_INTERVAL_SECS`: (Default: `30`) Indexer tick interval.
- `SUMMARIZATION_ENABLED`: (Default: `true`) Enable background rolling summarization.
- `SUMMARIZATION_INTERVAL_SECS`: (Default: `3600`) Summarization scheduler tick interval.
- `SUMMARIZATION_MAX_TOKENS`: (Default: `1200`) Hard cap for stored channel summaries (approximate).
- `CONTEXT_RETENTION_HOURS`: (Default: `24`) Short-term time filter; set to `0` to disable time filtering and rely on message count.

## Platform Notes

- Core bot code is OS-agnostic Rust; supported targets are macOS and Linux.
- Ensure the Rust toolchain is installed; other component dependencies must be on `PATH`.

## Interfaces

- **External**: Discord Gateway WebSocket.
- **Internal**: Provides `Data` struct to all commands via Poise context.

## State Management

Uses Poise's shared `Data` struct (thread-safe, wrapped in `Arc` by the framework), containing:

- `Config`: Loaded environment settings.
- `LlmClient`: Connection to LLM provider.
- `Database`: SQLite message and embedding storage.
- `MessageCache`: In-memory LRU cache of recent messages.
- `ToolRegistry`: Registry of callable tools.
- `McpClientManager`: Manager for MCP server connections.

## Error Handling

- Centralized Poise `on_error` handler logs errors and sends a user-facing response for command failures.
- Event handling logs persistence failures (no silent DB errors).

## Security

Commands restricted to the bot owner use the `owner_id` check from `src/config.rs`. Sensitive configuration fields (tokens, API keys) are redacted in `Debug` logs via a custom implementation in `src/config.rs`.
