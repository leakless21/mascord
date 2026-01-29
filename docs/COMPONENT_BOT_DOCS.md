# Component: Core Bot Documentation

## Area of Responsibility
General bot setup, command registration, and event lifecycle management.

## Key Classes / Modules
- `src/main.rs`: Entry point and Poise framework initialization.
- `src/config.rs`: Configuration handling (env vars, constants).
- `src/commands/mod.rs`: Command registration and grouping.

## Configuration & Environment
The bot is configured via environment variables (see `.env.example`). If a variable is missing, the bot uses sensible internal defaults defined in `src/config.rs`.
- `DISCORD_TOKEN`: **Required**. Bot token from Discord Developer Portal.
- `APPLICATION_ID`: **Required**. Discord application ID.
- `OWNER_ID`: (Optional) ID of the bot owner for admin-restricted commands.
- `LLAMA_URL`: (Default: `http://localhost:8080`) Base URL for the LLM API.
- `EMBEDDING_URL`: (Default: `LLAMA_URL`) Base URL for the embedding API.
- `SYSTEM_PROMPT`: (Default: Detailed agent prompt) The core instruction for the assistant.
- `YOUTUBE_COOKIES`: (Optional) Path to cookies file for `yt-dlp`.

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

## Security
Commands restricted to the bot owner use the `owner_id` check from `src/config.rs`. Sensitive configuration fields (tokens, API keys) are redacted in `Debug` logs via a custom implementation in `src/config.rs`.
