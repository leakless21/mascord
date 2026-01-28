# Requirements: Mascord Discord Bot

## Functional Requirements

### 1. Discord Interface
- Support Slash commands for all major features (LLM, Music, RAG).
- Event handling for message tracking (for RAG).
- Multi-channel support.

### 2. LLM Assistant (Chat)
- Integrated with local `llama.cpp` server via OpenAI-compatible API.
- Conversation context maintenance (per-channel).
- Streaming responses (if supported by Discord/Framework).

### 3. RAG (Retrieval-Augmented Generation)
- Store Discord messages in a local SQLite database.
- Generate embeddings using `llama.cpp` embedding endpoint.
- Vector search using `sqlite-vec`.
- Access controls:
  - Filter by channel(s).
  - Filter by date range.
  - Limit to latest XX days.

### 4. YouTube Audio Playback
- Voice channel connection/disconnection.
- Play audio from YouTube URLs using `yt-dlp`.
- Basic controls: Pause, Resume, Skip, Stop, Queue list.
- Songbird-based native implementation for low footprint.

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
