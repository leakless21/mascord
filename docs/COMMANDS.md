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

**Agent tools**: The same `/chat` flow can invoke built-in tools (history search, web tools when configured, music, etc.). Some tools show a confirmation button first; timeout is controlled by `AGENT_CONFIRM_TIMEOUT_SECS` (and per-guild overrides via `/settings agent_timeout`).

---

## Search & Memory Commands

### `/search [query]`

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

**Related commands** (moderators, **Manage Server**): use **`/settings memory`** to enable/disable tracking per channel, set scope, list settings, or purge stored messages (see below).

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

### `/reminder`

**Description**: Create and manage one-time reminders.

Set a reminder directly by providing `when` and `message`.

```
/reminder when:in 2 days, 30 minutes message:"Follow up with the team"
/reminder when:3 hours message:"Check the deployment"
/reminder when:at 5:30PM message:"Stretch break"
/reminder when:at 22:15 message:"Wrap up"
/reminder when:2026-02-10 17:30 message:"Daily standup"
```

Clock-style and absolute datetime inputs are interpreted as UTC.

Use `action` for reminder management:

#### List reminders

```
/reminder action:list
/reminder action:list limit:5
```

#### Cancel reminder

```
/reminder action:cancel reminder_id:42
```

#### Show help
Show accepted reminder formats and examples directly in Discord.

```
/reminder action:help
```

**Related Settings**:
- `REMINDER_POLL_INTERVAL_SECS` - Dispatcher polling interval
- `REMINDER_BATCH_SIZE` - Max reminders sent per poll cycle

---

## Music Commands

Requires **Connect** and **Speak** in the server. Use **`/join`** to join your current voice channel without playing; **`/play`** will auto-join if needed.

### `/join`

Join the voice channel you are currently in.

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

### `/skip`

Skip the current track (if any).

### `/leave`

Stop playback and disconnect from voice.

---

## Settings Commands

### `/settings [category]`

**Description**: Manage bot settings for your server.

**Categories**:

#### `/settings context get`

Show current context limit and retention for this server.

#### `/settings context set`

Update context settings (provide at least one option):

```
/settings context set limit:50 retention:48
```

- `limit` — max recent messages (1–100, enforced by the command)
- `retention` — hours of history to consider (1–168)

#### `/settings context summarize`

Manually run working-memory summarization for the current channel.

#### `/settings memory`

Per-channel **long-term** tracking (RAG persistence), moderator-only:

- **`list`** — channels with custom memory settings
- **`enable` / `disable`** — turn tracking on or off for a chosen channel
- **`scope`** — set a memory start date (or `none` for full history)
- **`purge`** — delete stored messages for a channel (confirm button; optional `before_date`)

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

## Admin commands (bot owner)

These slash commands are restricted to the configured **`OWNER_ID`** (hidden from public help).

### `/shutdown`

Gracefully disconnect from Discord and exit the process (under **systemd** or **`run-persistent.sh`**, the supervisor can restart the bot).

### `/restart`

Same as shutdown with a “restarting” message; use your process supervisor to bring the bot back.

---

## Discovering commands

Discord’s slash-command UI lists available commands and options. There is no separate `/help` command in this build.

---

## Command Categories

### 🧠 Conversation
- `/about` — Bot metadata
- `/chat` — Chat and agent tools (with confirmation when required)
- `/reminder` — Create, list, or cancel reminders

### 🔍 Memory & search
- `/search` — Query indexed message history
- `/memory` — Your global opt-in user memory
- `/settings memory` — Per-channel tracking, scope, purge (moderators)

### 🎵 Music
- `/join`, `/play`, `/skip`, `/leave`, `/queue`

### ⚙️ Settings
- `/settings context` — Context limit, retention, manual summarize
- `/settings system_prompt`, `agent_timeout`, `voice_timeout`

### 🔐 Owner
- `/shutdown`, `/restart`

---

## Tips & Tricks

- Want a one-off response without memory? Say things like **"no memory this time"** or **"temporary mode"** in your request.

### Combine requests in `/chat`

Ask for multi-step work in one message; the model may call tools (search, music, etc.) in sequence.

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

## Getting help

1. **In Discord**: Use the slash command picker; descriptions are embedded in each command.
2. **In the repo**: [README.md](../README.md), [DOCUMENTATION_INDEX.md](DOCUMENTATION_INDEX.md), and this file.
3. **Logs**: `journalctl -u mascord -f` (systemd) or your terminal if running in the foreground.

---

**Last updated**: March 28, 2026  
**Version**: Mascord 0.1.0
