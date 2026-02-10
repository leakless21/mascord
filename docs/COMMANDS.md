# Command Reference: Mascord

Complete list of available Mascord commands.

## Chat Commands

### `/chat [message]`

**Description**: Chat with the bot using current context memory.

**Usage**:
```
/chat Hello! How are you doing today?
/chat Tell me a joke
```

**What happens**:
1. Bot retrieves recent conversation history (default: last 50 messages)
2. Sends your message + context to the LLM
3. LLM generates a response
4. Response is sent back in Discord

**Options**:
- `message` (required): Your message for the bot

**Related Settings** (in `.env`):
- `CONTEXT_MESSAGE_LIMIT` - How many messages to include
- `CONTEXT_RETENTION_HOURS` - How old messages can be
- `SYSTEM_PROMPT` - Bot's personality

---

### `/agent [request]`

**Description**: Task the bot to perform a complex, multi-step action.

**Usage**:
```
/agent Search for the last time we discussed authentication, summarize it, and then play some lofi music
/agent Find recent messages about API design and create a summary
/agent Search the history and tell me what we decided about database schema
```

**What happens**:
1. Bot analyzes your request
2. Breaks it down into sub-tasks (search, summarize, execute)
3. Calls appropriate tools (RAG, music player, etc.)
4. Provides result

**Related Settings**:
- `MCP_TOOLS_REQUIRE_CONFIRMATION` - Require approval before executing tools
- `AGENT_CONFIRM_TIMEOUT_SECS` - How long to wait for confirmation

---

## Search & Memory Commands

### `/search [query]` (or `/rag search`)

**Description**: Search through message history (long-term memory).

**Usage**:
```
/search API authentication
/search when did we talk about the database
/search performance optimization tips
```

**What happens**:
1. Converts your query to embeddings
2. Searches stored message embeddings
3. Returns most relevant messages from history
4. Optionally summarizes results with LLM

**Range**:
- Can search months of history (if indexed)
- Filters by date and channel

**Related Commands**:
- `/rag enable` - Enable tracking for this channel
- `/rag disable` - Stop tracking this channel
- `/rag status` - Check tracking status

---

### `/rag [enable|disable|status|purge]`

**Description**: Manage long-term memory (Retrieval-Augmented Generation).

**Subcommands**:

#### `/rag enable`
Enable long-term message tracking for this channel.

```
/rag enable
```

#### `/rag disable`
Stop tracking messages in this channel (keeps existing messages).

```
/rag disable
```

#### `/rag status`
Show tracking status for this channel.

```
/rag status
```

#### `/rag purge [days]`
Delete old messages from this channel's history.

```
/rag purge 30      # Delete messages older than 30 days
/rag purge 0       # Delete all messages
```

**Related Settings**:
- `EMBEDDING_INDEXER_ENABLED` - Background message indexing
- `LONG_TERM_RETENTION_DAYS` - Auto-purge old messages

---

### `/memory [enable|disable|show|remember|forget|delete_data]`

**Description**: Manage your **global, opt-in** user memory profile (applies across servers and DMs).

**Subcommands**:

#### `/memory enable`
Enable your global memory profile.

```
/memory enable
```

#### `/memory disable`
Disable your global memory profile (keeps stored data).

```
/memory disable
```

#### `/memory show`
View your current memory profile and expiry status.

```
/memory show
```

#### `/memory remember [summary] [ttl_days]`
Create or replace your memory profile. `ttl_days` is optional.

```
/memory remember "I prefer concise answers and work in Rust." 90
```

#### `/memory forget`
Delete your memory profile only.

```
/memory forget
```

#### `/memory delete_data`
Delete your stored messages and memory profile (global).

```
/memory delete_data
```

---

## Reminder Commands

### `/reminder [set|list|cancel]`

**Description**: Create and manage one-time reminders.

**Subcommands**:

#### `/reminder set [when] [message]`
Set a reminder using a human-friendly duration.

```
/reminder set 10m "Stretch break"
/reminder set 2h "Check the deployment"
/reminder set 1d 2h "Follow up with the team"
```

#### `/reminder list [limit]`
List upcoming reminders (default 10, max 20).

```
/reminder list
/reminder list 5
```

#### `/reminder cancel [id]`
Cancel a pending reminder by ID (from `/reminder list`).

```
/reminder cancel 42
```

**Related Settings**:
- `REMINDER_POLL_INTERVAL_SECS` - Dispatcher polling interval
- `REMINDER_BATCH_SIZE` - Max reminders sent per poll cycle

---

## Music Commands

### `/play [url|search_term]`

**Description**: Play audio from YouTube or other supported sources.

**Usage**:
```
/play https://www.youtube.com/watch?v=dQw4w9WgXcQ
/play lofi hip hop study beats
/play https://youtu.be/dQw4w9WgXcQ
```

**Requirements**:
- You must be in a voice channel
- Bot must have permissions to connect and speak

**What happens**:
1. Bot joins your voice channel
2. Downloads audio using `yt-dlp`
3. Queues and starts playback
4. Shows interactive queue with controls

**Related Settings**:
- `YOUTUBE_COOKIES` - Path to cookies file for age-restricted content
- `YOUTUBE_DOWNLOAD_DIR` - Cache location for downloaded audio
- `YOUTUBE_CLEANUP_AFTER_SECS` - How long to keep cached files

---

### `/queue`

**Description**: Display the music queue with playback controls.

**Usage**:
```
/queue
```

**Controls**:
- ⏸️ **Pause** - Pause current playback
- ▶️ **Resume** - Resume paused playback
- ⏭️ **Skip** - Skip to next song
- ⏹️ **Stop** - Stop playback and clear queue

**Shows**:
- Currently playing song
- Upcoming songs
- Total queue duration
- Interactive buttons for control

---

### `/volume [level]`

**Description**: Adjust playback volume.

**Usage**:
```
/volume 50    # 50% volume
/volume 100   # 100% volume
/volume 10    # 10% volume (very quiet)
```

**Range**: 0-200 (0 = mute, 100 = normal, 200 = maximum)

---

## Settings Commands

### `/settings [category]`

**Description**: Manage bot settings for your server.

**Categories**:

#### `/settings context`
Manage conversation context and memory limits.

```
/settings context limit 100        # Remember last 100 messages
/settings context retention 48     # Remember messages from last 48 hours
/settings context summarize        # Manually trigger working memory summarization
```

**Options**:
- `limit` - Maximum recent messages to include (1-500)
- `retention` - Hours of messages to keep (0-unlimited)
- `summarize` - Create a summary of conversation history

#### `/settings system_prompt`
View or update the assistant's system prompt for this server.

```
/settings system_prompt                    # View current prompt
/settings system_prompt "Be concise"       # Set override
/settings system_prompt reset:true         # Reset to default
```

#### `/settings agent_timeout`
View or update the tool confirmation timeout.

```
/settings agent_timeout                 # View current timeout
/settings agent_timeout 180             # Set to 180 seconds
/settings agent_timeout reset:true      # Reset to default
```

#### `/settings voice_timeout`
View or update the voice idle timeout for auto-disconnect.

```
/settings voice_timeout                 # View current timeout
/settings voice_timeout 600             # Set to 10 minutes
/settings voice_timeout reset:true      # Reset to default
```


---

## Admin Commands

### `/admin [command]`

**Description**: Administrator-only commands (requires owner ID).

**Subcommands**:

#### `/admin shutdown`
Gracefully shut down the bot.

```
/admin shutdown
```

**What happens**:
1. Bot saves current state
2. Closes database connections
3. Disconnects from Discord
4. Exits cleanly

#### `/admin reload`
Reload configuration from `.env`.

```
/admin reload
```

#### `/admin status`
Show bot status and statistics.

```
/admin status
```

---

## Help Commands

### `/help`

**Description**: Show all available commands and their descriptions.

**Usage**:
```
/help
```

**Shows**:
- List of all commands
- Brief description of each
- How to get more help

### `/help [command]`

**Description**: Get detailed help for a specific command.

**Usage**:
```
/help chat
/help agent
/help play
```

---

## Command Categories

### 🧠 Conversation
- `/chat` - Chat with the bot
- `/agent` - Multi-step task execution
- `/help` - Get help

### 🔍 Memory & Search
- `/search` - Query message history
- `/rag enable/disable/status/purge` - Manage long-term memory
- `/memory enable/disable/show/remember/forget/delete_data` - Manage your global user memory

### 🎵 Music
- `/play` - Play audio from URL or search
- `/queue` - Show music queue
- `/volume` - Adjust volume

### ⚙️ Settings
- `/settings context` - Configure memory
- `/settings advanced` - Advanced options

### 🔐 Admin
- `/admin shutdown` - Graceful shutdown
- `/admin reload` - Reload config
- `/admin status` - Show status

---

## Tips & Tricks

- Want a one-off response without memory? Say things like **"no memory this time"** or **"temporary mode"** in your request.

### Combine Commands

You can chain requests:
```
/agent Search for our API design notes, summarize them, then play some music
```

This triggers `/search` + summarization + `/play` automatically.

### Use Reply Feature

Click "Reply" on any bot message to continue that conversation:
```
You:  /chat Tell me about async/await
Bot:  [Response]
You:  [Click Reply] How do I handle errors?
Bot:  [Responds in context of previous answer]
```

### Reference History

Ask the bot to search and reference:
```
/chat What did I say last week about the database?
(Bot will search history and include relevant messages)
```

### Music Queue Shortcuts

- React with ⏸️ to pause
- React with ⏭️ to skip
- React with ⏹️ to stop

---

## Permissions Required

The bot needs these Discord permissions to function fully:

- **Send Messages** - Send responses
- **Read Messages** - See messages to respond to
- **Connect** - Join voice channels
- **Speak** - Play audio
- **Manage Messages** - Delete old messages (optional)
- **Embed Links** - Send formatted responses
- **Attach Files** - Share files if needed

---

## Error Messages

| Error | Solution |
|-------|----------|
| "Command not found" | Command not registered - set `REGISTER_COMMANDS=true`, restart, then back to false |
| "Bot is not in a voice channel" | Join a voice channel first, then use `/play` |
| "Could not connect to LLM" | Check `LLAMA_URL` is correct and LLM server is running |
| "Database error" | Try restarting bot or clearing `data/mascord.db` |
| "Permission denied" | Check bot has required Discord permissions |

---

## Getting Help

1. **In Discord**: Type `/help` or `/help [command_name]`
2. **In Project**: See [README.md](../README.md) and [docs/](.)
3. **Check Logs**: Look for error messages: `tail -f /tmp/mascord.log`

---

**Last Updated**: February 4, 2026
**Version**: Mascord 0.1.0
