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
- **Rust Toolchain**: Required to build and run
- **yt-dlp**: Required for YouTube metadata and audio
- **FFmpeg**: Required for audio processing
- **CMake**: Required for building native libraries
- **LLM Provider**: Any OpenAI-compatible API (e.g., `llama.cpp` server, LocalAI, vLLM, or OpenAI)
- **Node.js** (optional): Required only if using MCP servers via `npx`

#### macOS Installation (Recommended)

On macOS, use **Homebrew** to install all dependencies automatically:

```bash
# Install all dependencies at once
brew install rustup ffmpeg yt-dlp node cmake opus pkg-config

# Initialize Rust
rustup default stable
```

Verify installation:
```bash
rustc --version
cargo --version
ffmpeg -version | head -1
yt-dlp --version
node --version
```

Or use the provided `Brewfile` for reproducible installs:
```bash
brew bundle  # Installs all dependencies from Brewfile
```

#### Linux Installation

Use your system's package manager:
```bash
# Ubuntu/Debian
sudo apt-get install rustup ffmpeg yt-dlp nodejs cmake libopus-dev pkg-config

# macOS (alternative if Homebrew not available)
sudo port install rust ffmpeg yt-dlp nodejs cmake libopus
```

#### Platform Notes

- **macOS**: Fully supported on Apple Silicon (ARM64) and Intel. All dependencies available via Homebrew.
- **Linux**: Supported on most distributions. See SETUP_MACOS.md for macOS-specific notes.
- **MCP servers**: If you use `npx`-based MCP servers, ensure Node.js is installed.

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

#### Environment Variable Reference (Commented)
Below is a commented reference you can copy into `.env` as needed:
```bash
# --- Discord ---
DISCORD_TOKEN=your_token_here                  # Required: Discord bot token
APPLICATION_ID=123456789012345678             # Required: Discord application ID
OWNER_ID=123456789012345678                    # Optional: owner-only admin commands

# --- LLM ---
LLAMA_URL=http://localhost:8080/v1             # Required: OpenAI-compatible API base (must include /v1)
LLAMA_MODEL=local-model                        # Chat model name
LLAMA_API_KEY=optional_key_here                # Optional: API key (if provider requires it)

# --- Embeddings ---
EMBEDDING_URL=http://localhost:8080/v1         # Defaults to LLAMA_URL if not set
EMBEDDING_MODEL=local-model                    # Embedding model name
EMBEDDING_API_KEY=optional_key_here            # Optional: API key for embeddings

# --- Storage ---
DATABASE_URL=data/mascord.db                   # SQLite DB location

# --- Bot behavior ---
SYSTEM_PROMPT=...                              # System prompt for the assistant
STATUS_MESSAGE=Ready to assist!                # Discord status message

# --- Memory (Short-term context) ---
CONTEXT_MESSAGE_LIMIT=50                       # Max recent messages injected into LLM
CONTEXT_RETENTION_HOURS=24                     # Retention window; set 0 to disable time filter

# --- Summarization (Working memory) ---
SUMMARIZATION_ENABLED=true
SUMMARIZATION_INTERVAL_SECS=3600               # Scheduler tick interval
SUMMARIZATION_ACTIVE_CHANNELS_LOOKBACK_DAYS=7  # Channels considered "active"
SUMMARIZATION_INITIAL_MIN_MESSAGES=50          # Minimum activity before first summary
SUMMARIZATION_TRIGGER_NEW_MESSAGES=150         # Trigger: new messages threshold
SUMMARIZATION_TRIGGER_AGE_HOURS=6              # Trigger: age threshold
SUMMARIZATION_TRIGGER_MIN_NEW_MESSAGES=20      # Trigger: min new msgs when age threshold is hit
SUMMARIZATION_MAX_TOKENS=1200                  # Approx token cap for summary
SUMMARIZATION_REFRESH_WEEKS=6                  # Periodic refresh cadence
SUMMARIZATION_REFRESH_DAYS_LOOKBACK=14         # Lookback window for refresh

# --- Embedding indexer (Long-term memory) ---
EMBEDDING_INDEXER_ENABLED=true                 # Background embedding backfill
EMBEDDING_INDEXER_BATCH_SIZE=25                # Messages per batch
EMBEDDING_INDEXER_INTERVAL_SECS=30             # Seconds between batches

# --- Long-term memory retention (RAG store) ---
LONG_TERM_RETENTION_DAYS=365                   # Set 0 to disable cleanup

# --- Agent tool confirmation ---
AGENT_CONFIRM_TIMEOUT_SECS=300                 # Confirmation timeout (seconds)
MCP_TOOLS_REQUIRE_CONFIRMATION=true            # Require confirmation for MCP tools

# --- Timeouts ---
LLM_TIMEOUT_SECS=120
EMBEDDING_TIMEOUT_SECS=30
MCP_TIMEOUT_SECS=60

# --- Voice / YouTube ---
YOUTUBE_COOKIES=/path/to/cookies.txt           # Optional: cookies file for age-restricted content
YOUTUBE_DOWNLOAD_DIR=/tmp/mascord_audio        # yt-dlp download cache
YOUTUBE_CLEANUP_AFTER_SECS=3600                # Cleanup window for cached audio
VOICE_IDLE_TIMEOUT_SECS=300                    # Auto-leave voice after idle

# --- Command registration ---
REGISTER_COMMANDS=false                        # Set true only when commands change
DEV_GUILD_ID=YOUR_DEV_GUILD_ID_HERE            # Optional: faster dev registration
```

### 3. Database Setup

**Do I need to setup a database first?**
**No.** Mascord uses SQLite and handles its own database initialization. On the first run, the bot will automatically:
1. Create the `data` directory (if it doesn't exist).
2. Create the SQLite database file at the path specified in `DATABASE_URL`.
3. Initialize all necessary tables and indexes.

The `data/` directory is gitignored and will never be committed.

### 4. Build the Project

Build the bot once to create the optimized binary:

```bash
cd /path/to/mascord
cargo build --release    # Optimized binary (recommended)
cargo build              # Debug binary (faster compilation)
```

The release binary is located at: `target/release/mascord`

### 5. Running the Bot

⚠️ **CRITICAL: Avoid Rate Limits!**

Before starting your bot, ensure command registration is configured correctly to avoid Cloudflare IP bans:

```bash
# In .env - Set to false by default!
REGISTER_COMMANDS=false
DEV_GUILD_ID=your_test_server_id
```

**Why?** Setting `REGISTER_COMMANDS=true` causes the bot to register commands on **every startup**. Frequent restarts can trigger Discord's Cloudflare protection (**1+ hour IP ban**).

**When to enable command registration:**
- ✅ First time running the bot
- ✅ When you've modified command signatures (added/removed commands or parameters)
- ❌ Normal development restarts

**Best practice workflow:**
1. Set `REGISTER_COMMANDS=true` and `DEV_GUILD_ID` to your test server
2. Start bot once to register commands
3. Set `REGISTER_COMMANDS=false`
4. Continue development normally

#### Quick Start (Recommended)

**Use the provided `bot.sh` script** for an easy setup-and-run experience:

```bash
# Run the bot (release mode - optimized and recommended)
./bot.sh

# Or debug mode (verbose logging)
./bot.sh debug
```

The script automatically:
- Creates the `data/` directory
- Verifies your `.env` configuration
- Builds the binary if needed
- Starts the bot

#### Manual Startup

If you prefer to run without the script:

```bash
# Ensure data directory exists
mkdir -p data/

# Run with cargo
cargo run --release     # Optimized (recommended)
cargo run               # Debug mode

# Or run the compiled binary directly
./target/release/mascord
```

📖 **For detailed information**, see [SETUP_COMPLETE.md](SETUP_COMPLETE.md) and [DATABASE_FIX.md](DATABASE_FIX.md)

### 6. MCP Servers Configuration

**MCP (Model Context Protocol)** servers extend the bot's capabilities with external tools and data sources.

#### Setup:
1. Copy the example configuration:
   ```bash
   cp mcp_servers.toml.example mcp_servers.toml
   ```

2. Edit `mcp_servers.toml` and add your actual API keys:
   ```toml
   [[servers]]
   name = "brave-search"
   transport = "http"
   command = "npx"
   args = ["-y", "@modelcontextprotocol/server-brave-search"]
   env = { BRAVE_API_KEY = "your_actual_api_key_here" }
   ```

> [!IMPORTANT]
> **Security**: The `mcp_servers.toml` file is gitignored and will never be committed. You can safely put your API keys directly in this file. The `mcp_servers.toml.example` template is committed to the repo for reference.

#### Available MCP Servers:
- **brave-search**: Web search powered by Brave Search API
- **fetch**: HTTP client for fetching web content

---

## � Monitoring Your Bot

### Check if Bot is Running

```bash
# See running Mascord process
ps aux | grep mascord

# Check database message count
sqlite3 data/mascord.db "SELECT COUNT(*) as messages FROM messages;"

# View real-time logs (if running in background)
tail -f /tmp/mascord.log
```

### Stop the Bot

```bash
# Graceful shutdown (saves state)
pkill -f "target/release/mascord"

# Or in Discord, use (owner-only):
/admin shutdown
```

### Troubleshooting

| Issue | Solution |
|-------|----------|
| Bot won't start | `mkdir -p data/` and verify `.env` has `DISCORD_TOKEN` and `APPLICATION_ID` |
| "Failed to open database" | `mkdir -p data/` - SQLite needs parent directory to exist |
| Database errors | `rm data/mascord.db` (will be recreated on startup) |
| LLM connection fails | Check `LLAMA_URL` is accessible: `curl $LLAMA_URL/models` |
| Bot doesn't respond to commands | Verify bot has Discord permissions (Send Messages, Read Messages) |
| High memory usage | Reduce `CONTEXT_MESSAGE_LIMIT` or increase `LONG_TERM_RETENTION_DAYS` |
| Commands not appearing | Set `REGISTER_COMMANDS=true`, restart once, then set back to false |

---

## 🎮 Usage Guide

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

Start with the most relevant guide for your needs:

- **[Quick Start Guide](docs/QUICK_START.md)** - Get running in 5 minutes (⭐ start here!)
- **[Installation Guide](docs/INSTALLATION.md)** - Full setup with troubleshooting
- **[Command Reference](docs/COMMANDS.md)** - All available commands with examples
- **[Documentation Index](docs/DOCUMENTATION_INDEX.md)** - Complete documentation map

### Technical Documentation

For deeper insights into the project, explore the `docs/` directory:

- [Architecture](docs/ARCHITECTURE.md): System design, component overview, and data flow.
- [Requirements](docs/REQUIREMENTS.md): Detailed functional and non-functional goals.
- [Component Docs](docs/COMPONENT_BOT_DOCS.md): Deep dives into specific modules (Bot, LLM, RAG, Voice, Tools).

### Setup & Troubleshooting

- [SETUP_COMPLETE.md](SETUP_COMPLETE.md): Post-setup verification and next steps
- [DATABASE_FIX.md](DATABASE_FIX.md): Database initialization and troubleshooting
- [QUICK_REFERENCE.md](QUICK_REFERENCE.md): Command cheat sheet

---

## 🤝 Contribution

Mascord follows a modular architecture. Feel free to contribute by adding new tools to `src/tools/` or extending the Agentic capabilities via new MCP server integrations.

---

*Built with ❤️ using Serenity, Poise, and Songbird.*
