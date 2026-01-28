# Requirements: Mascord Discord Bot

## Functional Requirements

### 1. Discord Interface
- Support Slash commands for all major features (LLM, Music, RAG).
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

### 4. YouTube Audio Playback
- Voice channel connection/disconnection.
- Play audio from YouTube URLs using `yt-dlp`.
- Support for `youtube-cookies` to bypass bot/age detection.
- Basic controls: Pause, Resume, Skip, Stop, Queue list.
- Songbird-based native implementation for low footprint.

### 5. Natural Language Task Execution (Agent)
- Orchestrate complex tasks using natural language.
- Execute built-in tools (Music, RAG, Admin).
- Integrate external tools via Model Context Protocol (MCP).
- Multi-turn tool calling loop for autonomous problem solving.
- Configurable iteration limits and safety checks.

### 6. Administration & Security
- Admin-only commands restricted by `OWNER_ID`.
- Secure handling of API keys for LLM and Embedding services.
- Graceful shutdown triggered by authorized users.

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
