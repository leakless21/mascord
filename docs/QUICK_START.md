# Quick Start Guide: Mascord

Get Mascord up and running in 10 minutes on macOS!

## ⚡ Ultra-Quick Start (5 minutes)

### 1. Install Dependencies (1 min)

```bash
brew install rustup ffmpeg yt-dlp node cmake opus pkg-config
rustup default stable
```

### 2. Configure Bot (2 min)

```bash
cd /path/to/mascord
cp .env.example .env
# Edit .env and set:
#   DISCORD_TOKEN = (from Discord Developer Portal)
#   APPLICATION_ID = (from Developer Portal)
```

### 3. Run Bot (2 min)

```bash
./bot.sh
```

**Done!** Your bot is now online in Discord. 🎉

---

## 📋 Essential Configuration

Only 3 things are truly required:

### 1. DISCORD_TOKEN

Get from [Discord Developer Portal](https://discord.com/developers/applications):
- Create Application → Add Bot → Copy Token
- Set in `.env`: `DISCORD_TOKEN=your_token_here`

### 2. APPLICATION_ID

Also from Developer Portal:
- General Information tab
- Set in `.env`: `APPLICATION_ID=123456789`

### 3. LLAMA_URL (Optional but Recommended)

Choose one:

**Option A: Use OpenRouter (free tier available)**
```bash
# Set in .env
LLAMA_URL=https://openrouter.ai/api/v1
LLAMA_API_KEY=sk-or-v1-...  # from openrouter.ai
LLAMA_MODEL=nvidia/nemotron-3-nano-30b-a3b:free
```

**Option B: Use Local LLM (llama.cpp)**
```bash
# Terminal 1: Start LLM server
llama-server -m model.gguf -ngl 99 --port 8080

# Terminal 2: Set in .env
LLAMA_URL=http://localhost:8080/v1
LLAMA_MODEL=local-model
```

**Option C: Use OpenAI API**
```bash
# Set in .env
LLAMA_URL=https://api.openai.com/v1
LLAMA_API_KEY=sk-...
LLAMA_MODEL=gpt-3.5-turbo
```

---

## 🚀 Running the Bot

### Start Bot

```bash
./bot.sh              # Release (recommended, fast)
./bot.sh debug        # Debug mode (verbose logging)
```

### Check Status

```bash
# Is it running?
ps aux | grep mascord

# See database
sqlite3 data/mascord.db "SELECT COUNT(*) FROM messages;"

# Check logs
tail -f /tmp/mascord.log
```

### Stop Bot

```bash
pkill -f "target/release/mascord"
```

---

## 💬 Test Your Bot

In Discord:

1. **Start a conversation**
   ```
   /chat Hello, how are you?
   ```

2. **Ask it to play music**
   ```
   /play https://www.youtube.com/watch?v=...
   ```

3. **Enable long-term memory**
   ```
   /rag enable
   ```

---

## 🔧 Common Settings

Edit `.env` to customize:

```bash
# Bot personality
SYSTEM_PROMPT="You are a helpful Discord bot"
STATUS_MESSAGE="Ready to assist!"

# Memory size (number of recent messages to remember)
CONTEXT_MESSAGE_LIMIT=50

# Keep messages for this long (hours)
CONTEXT_RETENTION_HOURS=24

# Auto-delete old messages after this many days
LONG_TERM_RETENTION_DAYS=365

# Summarize conversations
SUMMARIZATION_ENABLED=true
```

---

## 🐛 Troubleshooting

### Bot won't start

```bash
# Create data directory
mkdir -p data/

# Verify .env has required fields
grep DISCORD_TOKEN .env
grep APPLICATION_ID .env

# Try again
./bot.sh
```

### Bot doesn't respond

1. Check bot has Discord permissions (Send Messages, Read Messages)
2. Check LLM endpoint is working: `curl $LLAMA_URL/models`
3. Set `REGISTER_COMMANDS=true`, restart, then set back to false

### "Failed to open database"

```bash
mkdir -p data/
./bot.sh
```

---

## 📚 Next Steps

- [Full Installation Guide](INSTALLATION.md)
- [Complete README](../README.md)
- [Available Commands](../README.md#-available-commands)
- [Architecture Overview](ARCHITECTURE.md)

---

## 🎓 Learning Path

1. **Get bot running** ← You are here
2. **Read [README.md](../README.md)** - Understand features
3. **Explore [docs/](.)** - Deep dive into components
4. **Configure advanced settings** - See `.env` comments
5. **Set up MCP servers** - Extend with external tools

---

**Ready to go?** Run `./bot.sh` now! 🤖
