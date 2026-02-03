# Requirements: Mascord Discord Bot

## Functional Requirements

### 1. Discord Interface

- Support Slash commands for all major features (LLM, Music, RAG).
- **Reply-based Conversations**: Automatically respond when a user replies to the bot's messages.
- **Universal Embed Responses**: Use embeds for all bot responses to bypass Discord's 2000-character plain text limit.
- **R-007**: Fail fast on excessive Discord rate limits (e.g., > 60s) during startup to avoid unresponsive hanging.
- **R-008**: Provide clear, actionable error messages when external services (Discord, LLM, MCP) are unavailable or rate-limited.
- **R-009**: Surface command errors to users with a consistent, friendly response while logging full details.
- Event handling for message tracking (for RAG).
- Multi-channel support.

### 2. LLM Assistant (Chat)

- Integrated with local `llama.cpp` server via OpenAI-compatible API.
- Conversation context maintenance with Three-Tier Architecture:
    - **Short-Term**: Verbatim recent history.
    - **Working Memory**: Proactive summarization of older interactions.
    - **Long-Term**: On-demand search of full history.
- Streaming responses (if supported by Discord/Framework).

### 3. RAG (Retrieval-Augmented Generation)

- Store Discord messages in a local SQLite database.
- Generate embeddings using `llama.cpp` embedding endpoint.
- Vector search using `sqlite-vec`.
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

### 5. Natural Language Task Execution (Agent)

- Orchestrate complex tasks using natural language.
- Execute built-in tools (Music, RAG, Admin).
- Integrate external tools via Model Context Protocol (MCP) (e.g., Brave Search, Web Fetching).
- Multi-turn tool calling loop for autonomous problem solving.
- Configurable iteration limits and safety checks.


### 6. Administration & Security

- Admin-only commands restricted by `OWNER_ID`.
- Secure handling of API keys for LLM and Embedding services.
- Graceful shutdown triggered by authorized users.

### 7. Configuration & Deployment

- The bot must provide sensible defaults for all configuration variables (LLM URLs, ports, prompts) to ensure "zero-config" functionality beyond Discord credentials.
- All configuration should be overrideable via environment variables or a `.env` file.
- The system should maintain an up-to-date `.env.example` that matches these internal defaults.

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
