# Architecture: Mascord Discord Bot

## System Overview

Mascord is designed as a modular Discord bot focusing on local resource efficiency and clean separation of concerns.

## Components

### 1. Bot Framework (Poise/Serenity)
- **Responsibility**: Discord API interaction, gateway management, command dispatching.
- **Interface**: Uses `poise::Framework` for command routing. Unified `/chat` command for all interactions.
- **Dependencies**: Discord Gateway.

### 2. Audio Service (Songbird)
- **Responsibility**: Voice channel state management, audio streaming, queue handling.
- **Compute**: Low (audio decoding via Opus).
- **Interface**: `src/voice/player.rs`.
- **Dependencies**: `yt-dlp`, `ffmpeg`, optional `YOUTUBE_COOKIES`.

### 3. LLM Client (async-openai)
- **Responsibility**: Communicating with an LLM provider.
- **Support**: Works with *any* OpenAI-compatible API (e.g., `llama.cpp`, LocalAI, vLLM, Groq, or OpenAI).
- **Interface**: `src/llm/client.rs`.
- **Dependencies**: Configurable `LLAMA_URL` and optional `LLAMA_API_KEY`.

### 4. RAG Engine
- **Responsibility**: Message indexing, similarity search, prompt augmentation.
- **Storage**: SQLite (+ `sqlite-vec`).
- **Compute**: Moderate (vector arithmetic).
- **Interface**: `src/rag/mod.rs`.
- **Dependencies**: SQLite, `llama.cpp` (for embeddings), optional `EMBEDDING_API_KEY`.

### 5. Caching Layer (LruCache)
- **Responsibility**: Size-managed, thread-safe in-memory storage of recent Discord messages.
- **Efficiency**: Reduces SQLite/Discord API calls for frequently accessed data.
- **Interface**: `src/cache.rs`.
- **Dependencies**: `lru` crate.

### 6. Tool System
- **Responsibility**: Orchestrating function calls, managing built-in and external tools.
- **Interface**: `src/tools/`.
- **Dependencies**: `serde_json`.

### 7. MCP Manager
- **Responsibility**: Managing connections to external Model Context Protocol servers.
- **Interface**: `src/mcp/`.
- **Dependencies**: `rmcp`, `tokio`.

### 8. Context Manager (Three-Tier Memory)
- **Responsibility**: Orchestrating the bot's functional memory across three layers:
    - **Short-Term**: Last 50 verbatim messages (LruCache).
    - **Working Memory**: Condensed summaries of older conversations (Working Context).
    - **Long-Term Memory**: Indexed message history for tool-based retrieval (RAG).
- **Interface**: `src/context.rs`.
- **Dependencies**: `src/cache.rs`, `src/db/mod.rs`, `src/summarize.rs`.

### 9. Summarization Service
- **Responsibility**: Periodically condensing channel history into persistent summaries to maintain "Working Memory".
- **Compute**: Low (triggered every 4 hours, requires LLM call).
- **Interface**: `src/summarize.rs`.
- **Dependencies**: `src/llm/client.rs`, `src/db/mod.rs`.

## Data Storage

### SQLite Database
- **Location**: `data/mascord.db`
- **Tables**:
  - `messages`: Standard message history (guild_id, channel_id, user_id, content, timestamp).
  - `channel_summaries`: Condensed Working Memory snapshots (channel_id, summary, updated_at).
  - `settings`: Per-server/channel configurations.

## Interfaces

```mermaid
graph LR
    User([User]) <--> Discord[Discord API]
    Discord <--> Framework[Framework - src/main.rs]
    
    subgraph Services
        Framework --> ChatCmd[/chat Command]
        ChatCmd --> Agent[Agent Loop]
        Framework --> Voice[Voice Service - src/voice/]
        Framework --> RAG[RAG Service - src/rag/]
        Framework --> Cache[Caching Layer - src/cache.rs]
        Framework --> Tools[Tool System - src/tools/]
        Framework --> MCP[MCP Manager - src/mcp/]
    end
    
    LLM <--> LlamaServer[llama.cpp Server]
    Voice --> YTDLP[yt-dlp]
    RAG <--> DB[(SQLite)]
    Cache <--> Discord
    Tools --> Builtin[Built-in Tools]
    MCP <--> MCPServers[External MCP Servers]
```
