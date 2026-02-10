-- Standard message storage
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    discord_id TEXT NOT NULL UNIQUE,
    guild_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    is_indexed BOOLEAN DEFAULT FALSE,
    embedding BLOB NULL
);

-- Index for date and channel filtering
CREATE INDEX IF NOT EXISTS idx_messages_channel_date ON messages (channel_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_guild_date ON messages (guild_id, timestamp);

CREATE TABLE IF NOT EXISTS settings (
    guild_id TEXT PRIMARY KEY,
    context_limit INTEGER,
    context_retention INTEGER
);

CREATE TABLE IF NOT EXISTS channel_summaries (
    channel_id TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    refreshed_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS channel_settings (
    guild_id TEXT NOT NULL,
    channel_id TEXT PRIMARY KEY,
    enabled BOOLEAN DEFAULT TRUE,
    memory_start_date DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_channel_guild ON channel_settings (guild_id);

CREATE TABLE IF NOT EXISTS channel_milestones (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id TEXT NOT NULL,
    milestone TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_milestones_channel_created ON channel_milestones (channel_id, created_at);

CREATE TABLE IF NOT EXISTS reminders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    message TEXT NOT NULL,
    remind_at DATETIME NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    delivery_attempts INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    sent_at DATETIME,
    cancelled_at DATETIME,
    last_error TEXT
);
CREATE INDEX IF NOT EXISTS idx_reminders_due ON reminders (status, remind_at);
CREATE INDEX IF NOT EXISTS idx_reminders_user ON reminders (guild_id, user_id, status, remind_at);

-- Note: sqlite-vec setup usually involves virtual tables.
-- Mascord currently uses in-process Rust vector scoring over BLOB embeddings.
