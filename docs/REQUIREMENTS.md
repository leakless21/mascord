# Requirements: Mascord Discord Bot

## Functional Requirements

### 1. Discord Interface

- Support Slash commands for all major features (LLM, Music, RAG).
- **Reply-based Conversations**: Automatically respond when a user replies to the bot's messages.
- **Mention-based Conversations**: Automatically respond when a user mentions/tags the bot (no need for `/chat`).
- **Universal Embed Responses**: Use embeds for all bot responses to bypass Discord's 2000-character plain text limit.
- **R-007**: Fail fast on excessive Discord rate limits (e.g., > 60s) during startup to avoid unresponsive hanging.
- **R-008**: Provide clear, actionable error messages when external services (Discord, LLM, MCP) are unavailable or rate-limited.
- **R-009**: Surface command errors to users with a consistent, friendly response while logging full details.
- **R-010**: Convert Markdown responses into Discord-supported formatting, degrading unsupported elements (tables, images, HTML) into readable text.
- Event handling for message tracking (for RAG).
- Multi-channel support.

### 2. LLM Assistant (Chat)

- Integrated with local `llama.cpp` server via OpenAI-compatible API.
- Conversation context maintenance with Three-Tier Architecture:
    - **Short-Term**: Verbatim recent history.
    - **Working Memory**: Rolling summaries of older interactions with hard size caps, periodic refresh, and milestone anchors (durable facts/decisions).
    - **Long-Term**: On-demand search of full history.
- **Context Retention Modes**: `CONTEXT_RETENTION_HOURS=0` disables time filtering for short-term memory (count-only).
- Streaming responses (if supported by Discord/Framework).
- **User Memory (Opt-in)**:
  - Allow users to explicitly opt-in to a personal, curated memory profile (preferences, ongoing projects).
  - Provide commands to view, edit, and delete user memory.
  - Store a **global** user memory profile (applies across servers and DMs).
  - Inject a short memory snippet into prompts; allow the agent to fetch full details via a tool when needed.
  - Automatically update memory when enabled, using guardrails to avoid sensitive data.
  - Support temporary no-memory requests via natural language cues (do not use or update memory).
  - Support retention policies (TTL/expiry) and hard-delete on request.

### 3. RAG (Retrieval-Augmented Generation)

- Store Discord messages in a local SQLite database.
- Generate embeddings using `llama.cpp` embedding endpoint.
- Vector search using SQLite-stored embeddings with in-process Rust similarity scoring; `sqlite-vec` remains an optional acceleration path.
- Hybrid retrieval: merge vector + keyword results with a light recency boost to favor recent context.
- Access controls:
  - Filter by channel(s).
  - Filter by date range.
  - Limit to latest XX days.
- **Research Summarization**: Tool-based retrieval should provide condensed results using the LLM for better context density.
- **Memory Control**:
  - Enable/disable tracking per channel.
  - Set memory scope (start date) per channel.
  - Purge historical messages by channel or date.

### 4. YouTube Audio Playback

- Voice channel connection/disconnection.
- Play audio from YouTube URLs using `yt-dlp`.
- Support for `youtube-cookies` to bypass bot/age detection.
- Warn and continue without cookies if `YOUTUBE_COOKIES` points to a missing file.
- Basic controls: Pause, Resume, Skip, Stop, Queue list.
- Songbird-based native implementation for low footprint.

### 5. Reminders

- Allow users to set one-time reminders with human-friendly durations (e.g., minutes/hours/days).
- Persist reminders in SQLite so they survive restarts.
- Deliver reminders in the originating channel and mention the requesting user.
- Prevent mass mentions (`@everyone`, roles) from reminder text.
- Provide commands to list and cancel pending reminders.
- Throttle reminder dispatching to avoid Discord API rate limits.

### 6. Natural Language Task Execution (Agent)

- Orchestrate complex tasks using natural language.
- Execute built-in tools (Music, RAG, Admin).
- Integrate external tools via Model Context Protocol (MCP) (e.g., Brave Search, Web Fetching).
- Multi-turn tool calling loop for autonomous problem solving.
- Configurable iteration limits and safety checks.


### 7. Administration & Security

- Admin-only commands restricted by `OWNER_ID`.
- Secure handling of API keys for LLM and Embedding services.
- Graceful shutdown triggered by authorized users.
- Provide a user-facing mechanism to delete their stored data (messages and user memory) on request.

### 8. Configuration & Deployment

- The bot must provide sensible defaults for all configuration variables (LLM URLs, ports, prompts) to ensure "zero-config" functionality beyond Discord credentials.
- All configuration should be overrideable via environment variables or a `.env` file.
- The system should maintain an up-to-date `.env.example` that matches these internal defaults.
- Server owners must be able to override selected runtime settings via slash commands, with per-guild values persisted in the database (e.g., system prompt, agent confirmation timeout, voice idle timeout).

## Non-Functional Requirements

### 1. Performance

- Low memory footprint (aiming for < 100MB excluding LLM server).
- Fast response times for bot commands.
- Efficient vector search within SQLite.

### 2. Scalability

- Optimized for small number of servers (private use case).
- Prioritize efficiency over massive scale.

### 3. Maintainability

- Follow SOLID, KISS, and YAGNI principles.
- Clean project structure and thorough documentation.
- Rust code with strong type safety and error handling.

### 4. Reliability

- Graceful handling of `llama.cpp` server downtime.
- Robust error handling for `yt-dlp` failures.
- Database integrity for message history.

### 5. Platform Support

- The bot must build and run on macOS (Apple Silicon and Intel) and Linux with the documented prerequisites installed.
- External runtime dependencies (`yt-dlp`, `ffmpeg`, optional Node.js for MCP servers) must be documented with OS-specific install guidance.
- Avoid OS-specific code paths unless explicitly gated and documented.
