-- Standard message storage
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    discord_id TEXT NOT NULL UNIQUE,
    guild_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    is_indexed BOOLEAN DEFAULT FALSE
);

-- Index for date and channel filtering
CREATE INDEX IF NOT EXISTS idx_messages_channel_date ON messages (channel_id, timestamp);

-- Note: sqlite-vec setup usually involves virtual tables. 
-- We will implement the vector storage once we verify the extension loading.
