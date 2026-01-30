# Mascord: The Agentic Discord Assistant

**Mascord** is a high-performance, modular Discord bot written in Rust. It combines any **OpenAI-compatible LLM** (like `llama.cpp`, LocalAI, vLLM, or OpenAI itself), Retrieval-Augmented Generation (RAG), and a native music player with an advanced **Agentic** core powered by the Model Context Protocol (MCP).

---

## ✨ Key Features

- 🧠 **Three-Tier Persistent Memory**: Advanced context management featuring:
  - **Short-Term**: Verbatim recent conversation history (last 50 messages).
  - **Working Memory**: Condensing older interactions into persistent summaries via autonomous background jobs or manual triggers.
  - **Long-Term**: On-demand retrieval and LLM-powered summarization of historical messages (RAG).
- 🎵 **Interactive Music Player**: High-quality streaming with a rich UI:
  - **Interactive Queue**: Paginated queue display with control buttons (Pause, Skip, Stop).
  - **Deep Integration**: Uses `yt-dlp` and `songbird` with cookie support for detection bypass and age-restricted content.
- 🤖 **Agentic Core**: An autonomous agent trained to use internal and external tools (via MCP) to solve complex, multi-step requests.
- ⚙️ **Configurable Settings**: Per-guild configuration for context limits, retention policies, and manual working-memory refreshes.

---

## 🚀 Setup & Installation

### 1. Prerequisites

Mascord requires the following external tools for full functionality:
- **Rust Toolchain**: [Install Rust](https://rustup.rs/) (Required to build and run).
- **yt-dlp**: Required for YouTube metadata and audio. [Install yt-dlp](https://github.com/yt-dlp/yt-dlp#installation)
- **FFmpeg**: Required for audio processing. [Install FFmpeg](https://ffmpeg.org/download.html)
- **LLM Provider**: Any OpenAI-compatible API (e.g., `llama.cpp` server, LocalAI, vLLM, or OpenAI).

### 2. Configuration (`.env`)

Copy `.env.example` to `.env` and configure your credentials:
```bash
cp .env.example .env
```
Fill in the following essential variables:

| Variable | Description |
|----------|-------------|
| `DISCORD_TOKEN` | Your bot's token from the Discord Developer Portal. |
| `APPLICATION_ID` | Your bot's application ID. |
| `LLAMA_URL` | The OpenAI-compatible endpoint (e.g., `http://localhost:8080/v1`). **Must include `/v1`** for most local providers and **no trailing slash**. |
| `DATABASE_URL` | Path to the SQLite database (e.g., `data/mascord.db`). |

> [!TIP]
> Many more settings (timeouts, retention policies, YouTube settings, etc.) are available! Check the [`.env.example`](.env.example) file for the full list of configurable options.

### 3. Database Setup

**Do I need to setup a database first?**
**No.** Mascord uses SQLite and handles its own database initialization. On the first run, the bot will automatically:
1. Create the `data` directory (if it doesn't exist).
2. Create the SQLite database file at the path specified in `DATABASE_URL`.
3. Initialize all necessary tables and indexes.

### 4. Running the Bot

⚠️ **CRITICAL: Avoid Rate Limits!**

Before starting your bot, ensure you've configured command registration properly to avoid Cloudflare IP bans:

```bash
# In .env - Set to false by default!
REGISTER_COMMANDS=false
DEV_GUILD_ID=your_test_server_id
```

**Why?** Setting `REGISTER_COMMANDS=true` causes the bot to register commands on **every startup**. During development, frequent restarts can trigger Discord's Cloudflare protection, resulting in **1+ hour IP bans**.

**When to enable command registration:**
- ✅ First time running the bot
- ✅ When you've modified command signatures (added/removed commands or parameters)
- ❌ Normal development restarts

**Best practice workflow:**
1. Set `REGISTER_COMMANDS=true` and `DEV_GUILD_ID` to your test server
2. Start bot once to register commands
3. Set `REGISTER_COMMANDS=false`
4. Continue development normally

📖 **For detailed information**, see [docs/RATE_LIMIT_GUIDE.md](docs/RATE_LIMIT_GUIDE.md)

Once configured, start the bot:

```bash
cargo run
```

### 5. MCP Servers Configuration

**MCP (Model Context Protocol)** servers extend the bot's capabilities with external tools and data sources.

#### Setup:
1. Copy the example configuration:
   ```bash
   cp mcp_servers.toml.example mcp_servers.toml
   ```

2. Add your API keys to `.env`:
   ```bash
   # MCP Server Configuration
   BRAVE_API_KEY=your_brave_api_key_here
   ```

3. The MCP servers will automatically read API keys from your environment variables.

> [!IMPORTANT]
> **Security**: Never commit `mcp_servers.toml` with API keys! This file is gitignored. Always use environment variables for sensitive credentials.

#### Example Configuration:
```toml
[[servers]]
name = "brave-search"
transport = "http"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-brave-search"]
# BRAVE_API_KEY is read from environment

[[servers]]
name = "fetch"
transport = "stdio"
command = "uvx"
args = ["mcp-server-fetch"]
```


---

## 🛠️ Usage Guide

### 🧬 The Memory System
Mascord doesn't just "see" the last message. It manages context in three layers:
1. **Passive Observation**: The bot reads all messages (even without mentions) to maintain a live history.
2. **Conversation Context**: When you run `/chat`, it automatically pulls the last ~50 messages into the LLM's prompt.
3. **Working Memory**: For very long conversations, use `/settings context summarize`. This condenses the history into a "Working Memory" snippet that the bot always sees.
4. **Historical Search**: Use `/search` or tell the bot to "search for X" to trigger the RAG engine over months of historical logs.

### 🎵 Music Player Tips
- **Buttons**: The `/queue` command provides interactive buttons. You don't need to memorize commands once playback starts.
- **Cookies**: If you encounter `403 Forbidden` errors from YouTube, export your browser cookies to a `cookies.txt` and set the `YOUTUBE_COOKIES` path in your `.env`.

### 🤖 Multi-Step Tasks (Agent)
Use `/agent` for requests that require multiple actions. 
*Example: "Search for the last time we talked about the API design, summarize it, and then play some lofi music."*
## Future Capabilities (Roadmap)
- [ ] **Multimodal Search**: Index image attachments and embeds in RAG using CLIP/SigLIP models.
- [ ] **Vision Support**: Enable LLM to analyze images and screenshots shared in Discord.
- [ ] **Enhanced Audio**: Support for Spotify and local audio collections.
- [ ] **Web Search**: Integrated browsing tool for live information retrieval.
---

## 📋 Available Commands

| Command | Description |
|---------|-------------|
| `/chat` | Chat with the bot using current context memory. |
| `/search` | Manually search through the RAG database. |
| `/agent` | Task the bot to perform a complex, multi-step action. |
| `/play` | Stream audio from a YouTube URL. |
| `/queue` | View the interactive, paginated music player. |
| `/settings context` | Manage context limits or trigger common memory refreshes. |
| `/admin shutdown` | Safely save state and exit (Owner Only). |

---

## 📖 Documentation

For deeper insights into the project, explore the `docs/` directory:

- [Requirements](file:///home/lkless/project/code/mascord/docs/REQUIREMENTS.md): Detailed functional and non-functional goals.
- [Architecture](file:///home/lkless/project/code/mascord/docs/ARCHITECTURE.md): System design, component overview, and data flow.
- [Component Docs](file:///home/lkless/project/code/mascord/docs/COMPONENT_BOT_DOCS.md): Deep dives into specific modules (Bot, LLM, RAG, Voice, Tools).

---

## 🤝 Contribution

Mascord follows a modular architecture. Feel free to contribute by adding new tools to `src/tools/` or extending the Agentic capabilities via new MCP server integrations.

---

*Built with ❤️ using Serenity, Poise, and Songbird.*
