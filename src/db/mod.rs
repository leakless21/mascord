use crate::config::Config;
use anyhow::Context as AnyhowContext;
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vec.len() * 4);
    for f in vec {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

fn cosine_similarity_bytes(query: &[f32], query_norm: f32, candidate_bytes: &[u8]) -> f32 {
    if query.is_empty() || query_norm == 0.0 {
        return 0.0;
    }
    if candidate_bytes.len() != query.len() * 4 {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_b_sq = 0.0f32;

    for (i, chunk) in candidate_bytes.chunks_exact(4).enumerate() {
        let Ok(arr) = <[u8; 4]>::try_from(chunk) else {
            return 0.0;
        };
        let b = f32::from_le_bytes(arr);
        let a = query[i];
        dot += a * b;
        norm_b_sq += b * b;
    }

    let norm_b = norm_b_sq.sqrt();
    if norm_b == 0.0 {
        return 0.0;
    }
    dot / (query_norm * norm_b)
}

const RECENCY_WINDOW_DAYS: i64 = 30;
const RECENCY_MAX_BOOST: f32 = 0.05;

fn parse_sqlite_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn recency_boost(timestamp: &str, now: &DateTime<Utc>) -> f32 {
    let Some(ts) = parse_sqlite_timestamp(timestamp) else {
        return 1.0;
    };
    let age_secs = (now.signed_duration_since(ts).num_seconds()).max(0) as f32;
    let age_days = age_secs / 86_400.0;
    if age_days >= RECENCY_WINDOW_DAYS as f32 {
        1.0
    } else {
        let normalized = 1.0 - (age_days / RECENCY_WINDOW_DAYS as f32);
        1.0 + (RECENCY_MAX_BOOST * normalized)
    }
}

fn message_dedupe_key(msg: &crate::rag::MessageResult) -> String {
    format!(
        "{}|{}|{}|{}",
        msg.channel_id, msg.timestamp, msg.user_id, msg.content
    )
}

fn merge_results(
    primary: Vec<crate::rag::MessageResult>,
    secondary: Vec<crate::rag::MessageResult>,
    limit: usize,
) -> Vec<crate::rag::MessageResult> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    for msg in primary.into_iter().chain(secondary.into_iter()) {
        let key = message_dedupe_key(&msg);
        if seen.insert(key) {
            merged.push(msg);
            if merged.len() >= limit {
                break;
            }
        }
    }

    merged
}

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

pub struct ChannelSummaryRecord {
    pub summary: String,
    pub updated_at: String,
    pub refreshed_at: String,
}

impl Database {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let conn = Connection::open(&config.database_url)
            .with_context(|| format!("Failed to open database at '{}'", config.database_url))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock_conn(&self) -> anyhow::Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("Database connection lock poisoned"))
    }

    pub fn execute_init(&self) -> anyhow::Result<()> {
        info!("Database: Initializing schema...");
        let conn = self.lock_conn()?;

        // Base schema (idempotent).
        conn.execute_batch(
            "
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
            CREATE INDEX IF NOT EXISTS idx_milestones_channel_created
              ON channel_milestones (channel_id, created_at);
            ",
        )
        .context("Failed to initialize database schema")?;

        // Lightweight migrations. These must be safe to run repeatedly.
        // SQLite doesn't support `ADD COLUMN IF NOT EXISTS`, so we ignore "duplicate column name".
        if let Err(e) = conn.execute("ALTER TABLE messages ADD COLUMN embedding BLOB NULL", []) {
            let msg = e.to_string();
            if !msg.contains("duplicate column name") {
                return Err(e).context("Failed to migrate: add messages.embedding column");
            }
        }

        if let Err(e) = conn.execute(
            "ALTER TABLE channel_summaries ADD COLUMN refreshed_at DATETIME DEFAULT CURRENT_TIMESTAMP",
            [],
        ) {
            let msg = e.to_string();
            if !msg.contains("duplicate column name") {
                return Err(e).context("Failed to migrate: add channel_summaries.refreshed_at column");
            }
        }

        debug!("Database: Schema initialized successfully");
        Ok(())
    }

    pub fn save_message(
        &self,
        discord_id: &str,
        guild_id: &str,
        channel_id: &str,
        user_id: &str,
        content: &str,
        timestamp: i64,
    ) -> anyhow::Result<()> {
        debug!(
            "Database: Saving message {} from user {} in channel {}",
            discord_id, user_id, channel_id
        );
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO messages (discord_id, guild_id, channel_id, user_id, content, timestamp) 
             VALUES (?1, ?2, ?3, ?4, ?5, datetime(?6, 'unixepoch'))",
            (discord_id, guild_id, channel_id, user_id, content, timestamp),
        ).context("Failed to save message")?;
        Ok(())
    }

    pub fn get_messages_missing_embeddings(
        &self,
        limit: usize,
    ) -> anyhow::Result<Vec<(i64, String)>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, content
             FROM messages
             WHERE embedding IS NULL
               AND is_indexed = 0
               AND length(content) > 0
             ORDER BY timestamp DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map([limit], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn set_message_embedding(&self, message_id: i64, embedding: &[f32]) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        let embedding_blob = serialize_embedding(embedding);
        conn.execute(
            "UPDATE messages
             SET embedding = ?1,
                 is_indexed = 1
             WHERE id = ?2",
            (embedding_blob, message_id),
        )
        .context("Failed to update message embedding")?;
        Ok(())
    }

    pub fn mark_message_indexed(&self, message_id: i64) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE messages
             SET is_indexed = 1
             WHERE id = ?1",
            [message_id],
        )
        .context("Failed to mark message as indexed")?;
        Ok(())
    }

    pub fn set_guild_settings(
        &self,
        guild_id: u64,
        limit: Option<usize>,
        retention: Option<u64>,
    ) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;

        // Check if exists first
        let exists = conn
            .prepare("SELECT 1 FROM settings WHERE guild_id = ?1")?
            .exists([guild_id.to_string()])?;

        if exists {
            if let Some(l) = limit {
                conn.execute(
                    "UPDATE settings SET context_limit = ?1 WHERE guild_id = ?2",
                    (l, guild_id.to_string()),
                )?;
            }
            if let Some(r) = retention {
                conn.execute(
                    "UPDATE settings SET context_retention = ?1 WHERE guild_id = ?2",
                    (r, guild_id.to_string()),
                )?;
            }
        } else {
            conn.execute(
                "INSERT INTO settings (guild_id, context_limit, context_retention) VALUES (?1, ?2, ?3)",
                (guild_id.to_string(), limit, retention),
            )?;
        }
        Ok(())
    }

    pub fn get_guild_settings(
        &self,
        guild_id: u64,
    ) -> anyhow::Result<(Option<usize>, Option<u64>)> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare("SELECT context_limit, context_retention FROM settings WHERE guild_id = ?1")?;

        let mut rows = stmt.query([guild_id.to_string()])?;

        if let Some(row) = rows.next()? {
            let limit: Option<usize> = row.get(0).ok();
            let retention: Option<u64> = row.get(1).ok();
            Ok((limit, retention))
        } else {
            Ok((None, None))
        }
    }

    pub fn save_summary(&self, channel_id: &str, summary: &str) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO channel_summaries (channel_id, summary, updated_at) 
             VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(channel_id) DO UPDATE SET summary = ?2, updated_at = CURRENT_TIMESTAMP",
            (channel_id, summary),
        )?;
        Ok(())
    }

    pub fn save_summary_refresh(&self, channel_id: &str, summary: &str) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO channel_summaries (channel_id, summary, updated_at, refreshed_at)
             VALUES (?1, ?2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
             ON CONFLICT(channel_id) DO UPDATE
                 SET summary = ?2,
                     updated_at = CURRENT_TIMESTAMP,
                     refreshed_at = CURRENT_TIMESTAMP",
            (channel_id, summary),
        )?;
        Ok(())
    }

    pub fn get_latest_summary(&self, channel_id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.lock_conn()?;
        let mut stmt =
            conn.prepare("SELECT summary FROM channel_summaries WHERE channel_id = ?1")?;
        let mut rows = stmt.query([channel_id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_summary_record(
        &self,
        channel_id: &str,
    ) -> anyhow::Result<Option<ChannelSummaryRecord>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT summary, updated_at, refreshed_at
             FROM channel_summaries
             WHERE channel_id = ?1",
        )?;
        let mut rows = stmt.query([channel_id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(ChannelSummaryRecord {
                summary: row.get(0)?,
                updated_at: row.get(1)?,
                refreshed_at: row.get(2)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_channel_milestones(
        &self,
        channel_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT milestone
             FROM channel_milestones
             WHERE channel_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map((channel_id, limit), |row| row.get(0))?;

        let mut milestones = Vec::new();
        for row in rows {
            milestones.push(row?);
        }
        Ok(milestones)
    }

    pub fn replace_channel_milestones(
        &self,
        channel_id: &str,
        milestones: &[String],
    ) -> anyhow::Result<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;

        tx.execute(
            "DELETE FROM channel_milestones WHERE channel_id = ?1",
            [channel_id],
        )?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO channel_milestones (channel_id, milestone) VALUES (?1, ?2)",
            )?;

            for milestone in milestones {
                let trimmed = milestone.trim();
                if trimmed.is_empty() {
                    continue;
                }
                stmt.execute((channel_id, trimmed))?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_recent_messages(
        &self,
        channel_id: &str,
        from: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT m.content, m.user_id, m.timestamp, m.channel_id
             FROM messages m
             LEFT JOIN channel_settings s ON m.channel_id = s.channel_id
             WHERE m.channel_id = ?1
               AND (s.enabled IS NULL OR s.enabled = 1)
               AND (s.memory_start_date IS NULL OR m.timestamp >= s.memory_start_date)
               AND m.timestamp >= ?2
             ORDER BY m.timestamp DESC
             LIMIT ?3",
        )?;

        let from_str = from.format("%Y-%m-%d %H:%M:%S").to_string();
        let rows = stmt.query_map((channel_id, from_str, limit), |row| {
            Ok(crate::rag::MessageResult {
                content: row.get(0)?,
                user_id: row.get(1)?,
                timestamp: row.get(2)?,
                channel_id: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn get_channels_with_activity(&self, lookback_days: i64) -> anyhow::Result<Vec<String>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT channel_id
             FROM messages
             WHERE timestamp > datetime('now', ?1)
             GROUP BY channel_id
             ORDER BY MAX(timestamp) DESC",
        )?;

        let lookback = format!("-{} days", lookback_days);
        let rows = stmt.query_map([lookback], |row| row.get(0))?;

        let mut channels = Vec::new();
        for row in rows {
            channels.push(row?);
        }
        Ok(channels)
    }

    pub fn count_channel_messages_since(
        &self,
        channel_id: &str,
        since: &str,
    ) -> anyhow::Result<usize> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT COUNT(*)
             FROM messages
             WHERE channel_id = ?1
               AND timestamp > ?2",
        )?;
        let count: i64 = stmt.query_row((channel_id, since), |row| row.get(0))?;
        Ok(count.max(0) as usize)
    }

    // --- Channel Settings ---

    pub fn set_channel_enabled(
        &self,
        guild_id: &str,
        channel_id: &str,
        enabled: bool,
    ) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO channel_settings (guild_id, channel_id, enabled, updated_at) 
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(channel_id) DO UPDATE SET enabled = ?3, updated_at = CURRENT_TIMESTAMP",
            (guild_id, channel_id, enabled),
        )?;
        Ok(())
    }

    pub fn set_channel_memory_scope(
        &self,
        guild_id: &str,
        channel_id: &str,
        start_date: Option<String>,
    ) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO channel_settings (guild_id, channel_id, memory_start_date, updated_at) 
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(channel_id) DO UPDATE SET memory_start_date = ?3, updated_at = CURRENT_TIMESTAMP",
            (guild_id, channel_id, start_date),
        )?;
        Ok(())
    }

    pub fn get_channel_settings(
        &self,
        channel_id: &str,
    ) -> anyhow::Result<Option<(bool, Option<String>)>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT enabled, memory_start_date FROM channel_settings WHERE channel_id = ?1",
        )?;
        let mut rows = stmt.query([channel_id])?;

        if let Some(row) = rows.next()? {
            let enabled: bool = row.get(0)?;
            let scope: Option<String> = row.get(1)?;
            Ok(Some((enabled, scope)))
        } else {
            Ok(None)
        }
    }

    pub fn is_channel_tracking_enabled(&self, channel_id: &str) -> anyhow::Result<bool> {
        match self.get_channel_settings(channel_id)? {
            Some((enabled, _)) => Ok(enabled),
            None => Ok(true), // Default to enabled
        }
    }

    pub fn list_channel_settings(
        &self,
        guild_id: &str,
    ) -> anyhow::Result<Vec<(String, bool, Option<String>)>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare("SELECT channel_id, enabled, memory_start_date FROM channel_settings WHERE guild_id = ?1")?;
        let rows = stmt.query_map([guild_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn purge_messages(
        &self,
        channel_id: &str,
        before_date: Option<String>,
    ) -> anyhow::Result<usize> {
        let conn = self.lock_conn()?;
        let count = if let Some(date) = before_date {
            conn.execute(
                "DELETE FROM messages WHERE channel_id = ?1 AND timestamp < ?",
                (channel_id, date),
            )?
        } else {
            conn.execute("DELETE FROM messages WHERE channel_id = ?1", (channel_id,))?
        };
        Ok(count)
    }

    /// Removes messages older than `retention_hours` from the database.
    /// Returns the number of messages deleted.
    pub fn cleanup_old_messages(&self, retention_hours: u64) -> anyhow::Result<usize> {
        let conn = self.lock_conn()?;
        let count = conn.execute(
            "DELETE FROM messages WHERE timestamp < datetime('now', ?1)",
            (format!("-{} hours", retention_hours),),
        )?;
        Ok(count)
    }

    pub async fn search_messages(
        &self,
        query: &str,
        embedding: Vec<f32>,
        filter: crate::rag::SearchFilter,
    ) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
        let mut filter = filter;
        let limit = if filter.limit == 0 {
            5
        } else {
            filter.limit.min(100)
        };
        filter.limit = limit;

        if embedding.is_empty() {
            let results = self.search_messages_keyword(query, &filter)?;
            debug!(
                "Database: Keyword search returned {} results",
                results.len()
            );
            return Ok(results);
        }

        let db = self.clone();
        let query = query.to_string();
        tokio::task::spawn_blocking(move || {
            let vector_results = db.search_messages_vector(&embedding, &filter)?;
            let keyword_results = if query.trim().is_empty() {
                Vec::new()
            } else {
                db.search_messages_keyword(&query, &filter)?
            };

            Ok(merge_results(vector_results, keyword_results, limit))
        })
        .await?
    }

    fn search_messages_keyword(
        &self,
        query: &str,
        filter: &crate::rag::SearchFilter,
    ) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
        let conn = self.lock_conn()?;

        let mut sql = String::from(
            "
            SELECT m.content, m.user_id, m.timestamp, m.channel_id
            FROM messages m
            LEFT JOIN channel_settings s ON m.channel_id = s.channel_id
            WHERE (s.enabled IS NULL OR s.enabled = 1)
              AND (s.memory_start_date IS NULL OR m.timestamp >= s.memory_start_date)
            ",
        );

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if !query.is_empty() {
            sql.push_str(" AND m.content LIKE ?");
            params.push(Box::new(format!("%{}%", query)));
        }

        if !filter.channels.is_empty() {
            sql.push_str(" AND m.channel_id IN (");
            sql.push_str(&vec!["?"; filter.channels.len()].join(", "));
            sql.push(')');
            for channel in &filter.channels {
                params.push(Box::new(channel.clone()));
            }
        }

        if let Some(from) = filter.from_date {
            sql.push_str(" AND m.timestamp >= ?");
            params.push(Box::new(from.format("%Y-%m-%d %H:%M:%S").to_string()));
        }

        if let Some(to) = filter.to_date {
            sql.push_str(" AND m.timestamp <= ?");
            params.push(Box::new(to.format("%Y-%m-%d %H:%M:%S").to_string()));
        }

        sql.push_str(" ORDER BY m.timestamp DESC LIMIT ?");
        let limit = if filter.limit == 0 {
            5
        } else {
            filter.limit.min(100)
        };
        params.push(Box::new(limit));

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(&params_slice[..], |row| {
            Ok(crate::rag::MessageResult {
                content: row.get(0)?,
                user_id: row.get(1)?,
                timestamp: row.get(2)?,
                channel_id: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    fn search_messages_vector(
        &self,
        query_embedding: &[f32],
        filter: &crate::rag::SearchFilter,
    ) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
        const MAX_CANDIDATES: usize = 5000;

        let query_norm = query_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if query_norm == 0.0 {
            return Ok(Vec::new());
        }

        let conn = self.lock_conn()?;
        let now = Utc::now();

        let mut sql = String::from(
            "
            SELECT m.content, m.user_id, m.timestamp, m.channel_id, m.embedding
            FROM messages m
            LEFT JOIN channel_settings s ON m.channel_id = s.channel_id
            WHERE (s.enabled IS NULL OR s.enabled = 1)
              AND (s.memory_start_date IS NULL OR m.timestamp >= s.memory_start_date)
              AND m.embedding IS NOT NULL
            ",
        );

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if !filter.channels.is_empty() {
            sql.push_str(" AND m.channel_id IN (");
            sql.push_str(&vec!["?"; filter.channels.len()].join(", "));
            sql.push(')');
            for channel in &filter.channels {
                params.push(Box::new(channel.clone()));
            }
        }

        if let Some(from) = filter.from_date {
            sql.push_str(" AND m.timestamp >= ?");
            params.push(Box::new(from.format("%Y-%m-%d %H:%M:%S").to_string()));
        }

        if let Some(to) = filter.to_date {
            sql.push_str(" AND m.timestamp <= ?");
            params.push(Box::new(to.format("%Y-%m-%d %H:%M:%S").to_string()));
        }

        sql.push_str(" ORDER BY m.timestamp DESC LIMIT ?");
        params.push(Box::new(MAX_CANDIDATES));

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(&params_slice[..], |row| {
            Ok((
                crate::rag::MessageResult {
                    content: row.get(0)?,
                    user_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    channel_id: row.get(3)?,
                },
                row.get::<_, Vec<u8>>(4)?,
            ))
        })?;

        let mut scored = Vec::new();
        for row in rows {
            let (msg, embedding_bytes) = row?;
            let mut score = cosine_similarity_bytes(query_embedding, query_norm, &embedding_bytes);
            if score < 0.0 {
                score = 0.0;
            }
            if score > 0.0 {
                score *= recency_boost(&msg.timestamp, &now);
            }
            scored.push((score, msg));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let limit = if filter.limit == 0 {
            5
        } else {
            filter.limit.min(100)
        };
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, msg)| msg).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use chrono::{Duration, Utc};

    fn test_config() -> Config {
        Config {
            discord_token: "test".to_string(),
            application_id: 0,
            owner_id: Some(1),
            llama_url: "test".to_string(),
            llama_model: "test".to_string(),
            llama_api_key: None,
            embedding_url: "test".to_string(),
            embedding_model: "test".to_string(),
            embedding_api_key: None,
            database_url: ":memory:".to_string(),
            system_prompt: "test".to_string(),
            max_context_messages: 10,
            status_message: "test".to_string(),
            youtube_cookies: None,
            youtube_download_dir: "/tmp".to_string(),
            youtube_cleanup_after_secs: 3600,
            mcp_servers: Vec::new(),
            context_message_limit: 50,
            context_retention_hours: 24,
            llm_timeout_secs: 120,
            embedding_timeout_secs: 30,
            mcp_timeout_secs: 60,
            voice_idle_timeout_secs: 300,
            dev_guild_id: None,
            register_commands: false,
            mcp_tools_require_confirmation: true,
            agent_confirm_timeout_secs: 300,
            embedding_indexer_enabled: true,
            embedding_indexer_batch_size: 25,
            embedding_indexer_interval_secs: 30,
            summarization_enabled: true,
            summarization_interval_secs: 3600,
            summarization_active_channels_lookback_days: 7,
            summarization_initial_min_messages: 50,
            summarization_trigger_new_messages: 150,
            summarization_trigger_age_hours: 6,
            summarization_trigger_min_new_messages: 20,
            summarization_max_tokens: 1200,
            summarization_refresh_weeks: 6,
            summarization_refresh_days_lookback: 14,
            long_term_retention_days: 365,
        }
    }

    #[test]
    fn test_db_init_and_save() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        db.save_message("1", "g1", "c1", "u1", "hello", 1600000000)
            .unwrap();

        // Verify it exists (we don't have a direct get_message yet, but we can check query)
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT discord_id FROM messages WHERE discord_id = '1'")
            .unwrap();
        let exists = stmt.exists([]).unwrap();
        assert!(exists);
    }

    #[test]
    fn test_db_settings() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        // Test defaults (should be None)
        let (limit, retention) = db.get_guild_settings(123).unwrap();
        assert_eq!(limit, None);
        assert_eq!(retention, None);

        // Set settings
        db.set_guild_settings(123, Some(100), Some(48)).unwrap();

        let (limit, retention) = db.get_guild_settings(123).unwrap();
        assert_eq!(limit, Some(100));
        assert_eq!(retention, Some(48));

        // Update partial
        db.set_guild_settings(123, None, Some(72)).unwrap();
        let (limit, retention) = db.get_guild_settings(123).unwrap();
        assert_eq!(limit, Some(100)); // Should remain
        assert_eq!(retention, Some(72)); // Should update
    }

    #[test]
    fn test_mark_message_indexed_excludes_from_backfill() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        db.save_message("m1", "g1", "c1", "u1", "hi", 1700000000)
            .unwrap();

        let id_for = |discord_id: &str| -> i64 {
            let conn = db.conn.lock().unwrap();
            conn.query_row(
                "SELECT id FROM messages WHERE discord_id = ?1",
                [discord_id],
                |row| row.get(0),
            )
            .unwrap()
        };

        let pending = db.get_messages_missing_embeddings(10).unwrap();
        assert_eq!(pending.len(), 1);

        db.mark_message_indexed(id_for("m1")).unwrap();

        let pending = db.get_messages_missing_embeddings(10).unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_channel_settings() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        let guild_id = "123";
        let channel_id = "456";

        // Default should be enabled
        assert!(db.is_channel_tracking_enabled(channel_id).unwrap());

        // Test disable
        db.set_channel_enabled(guild_id, channel_id, false).unwrap();
        assert!(!db.is_channel_tracking_enabled(channel_id).unwrap());
        let (enabled, scope) = db.get_channel_settings(channel_id).unwrap().unwrap();
        assert!(!enabled);
        assert_eq!(scope, None);

        // Test scope
        let test_date = "2026-01-01 00:00:00".to_string();
        db.set_channel_memory_scope(guild_id, channel_id, Some(test_date.clone()))
            .unwrap();
        let (enabled, scope) = db.get_channel_settings(channel_id).unwrap().unwrap();
        assert!(!enabled); // Still disabled from before
        assert_eq!(scope, Some(test_date));

        // Test list
        let list = db.list_channel_settings(guild_id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, channel_id);

        // Test purge
        db.save_message("m1", guild_id, channel_id, "u1", "old", 1600000000)
            .unwrap();
        db.save_message("m2", guild_id, channel_id, "u1", "new", 1700000000)
            .unwrap();

        // Purge all
        let deleted = db.purge_messages(channel_id, None).unwrap();
        assert_eq!(deleted, 2);
    }

    #[test]
    fn test_rag_filtering_with_settings() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        let guild_id = "123";
        let c1 = "c1"; // Enabled
        let c2 = "c2"; // Disabled
        let c3 = "c3"; // With scope

        db.set_channel_enabled(guild_id, c2, false).unwrap();
        db.set_channel_memory_scope(guild_id, c3, Some("2026-01-01 00:00:00".to_string()))
            .unwrap();

        // Save messages
        db.save_message("m1", guild_id, c1, "u1", "msg in c1", 1700000000)
            .unwrap();
        db.save_message("m2", guild_id, c2, "u1", "msg in c2", 1700000000)
            .unwrap();
        db.save_message("m3", guild_id, c3, "u1", "old msg in c3", 1600000000)
            .unwrap(); // Before scope
        db.save_message("m4", guild_id, c3, "u1", "new msg in c3", 1800000000)
            .unwrap(); // After scope

        let filter = crate::rag::SearchFilter::default().with_limit(10);
        let results = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(db.search_messages("", vec![], filter))
            .unwrap();

        // Should return m1 and m4. m2 is disabled, m3 is out of scope.
        assert_eq!(results.len(), 2);
        let contents: Vec<_> = results.iter().map(|r| r.content.as_str()).collect();
        assert!(contents.contains(&"msg in c1"));
        assert!(contents.contains(&"new msg in c3"));
        assert!(!contents.contains(&"msg in c2"));
        assert!(!contents.contains(&"old msg in c3"));
    }

    #[test]
    fn test_replace_channel_milestones() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        let milestones = vec!["Decision A".to_string(), "Constraint B".to_string()];
        db.replace_channel_milestones("c1", &milestones).unwrap();

        let stored = db.get_channel_milestones("c1", 10).unwrap();
        assert_eq!(stored.len(), 2);
        assert!(stored.iter().any(|m| m == "Decision A"));
        assert!(stored.iter().any(|m| m == "Constraint B"));

        let new_milestones = vec!["New Plan".to_string()];
        db.replace_channel_milestones("c1", &new_milestones)
            .unwrap();

        let stored = db.get_channel_milestones("c1", 10).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0], "New Plan");
    }

    #[test]
    fn test_db_cleanup() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        // Save one very old message and one recent one
        // SQLite datetime('now') uses UTC
        // We use relative timestamps in the SQL, but for our mock we save with specific dates to test logic

        let conn = db.conn.lock().unwrap();
        // Insert manually to bypass save_message which converts unix to datetime
        conn.execute(
            "INSERT INTO messages (discord_id, guild_id, channel_id, user_id, content, timestamp) 
                     VALUES ('old', 'g1', 'c1', 'u1', 'old msg', datetime('now', '-48 hours'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (discord_id, guild_id, channel_id, user_id, content, timestamp) 
                     VALUES ('new', 'g1', 'c1', 'u1', 'new msg', datetime('now', '-1 hours'))",
            [],
        )
        .unwrap();
        drop(conn);

        // Retention is 24 hours
        let deleted = db.cleanup_old_messages(24).unwrap();
        assert_eq!(deleted, 1);

        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT discord_id FROM messages").unwrap();
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "new");
    }

    #[test]
    fn test_search_with_special_chars() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        db.save_message("1", "g1", "c1", "u1", "normal message", 1600000000)
            .unwrap();

        // This should NOT cause SQL injection
        let filter = crate::rag::SearchFilter::default().with_limit(10);
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(db.search_messages("'; DROP TABLE messages; --", vec![], filter));
        assert!(result.is_ok());

        // Verify table still exists
        let conn = db.conn.lock().unwrap();
        assert!(conn.prepare("SELECT 1 FROM messages").is_ok());
    }

    #[test]
    fn test_vector_search_ranks_by_similarity() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        // Insert three messages.
        db.save_message("m1", "g1", "c1", "u1", "alpha", 1700000000)
            .unwrap();
        db.save_message("m2", "g1", "c1", "u1", "bravo", 1700000001)
            .unwrap();
        db.save_message("m3", "g1", "c1", "u1", "charlie", 1700000002)
            .unwrap();

        // Attach small test embeddings.
        let id_for = |discord_id: &str| -> i64 {
            let conn = db.conn.lock().unwrap();
            conn.query_row(
                "SELECT id FROM messages WHERE discord_id = ?1",
                [discord_id],
                |row| row.get(0),
            )
            .unwrap()
        };

        db.set_message_embedding(id_for("m1"), &[1.0, 0.0, 0.0])
            .unwrap();
        db.set_message_embedding(id_for("m2"), &[0.0, 1.0, 0.0])
            .unwrap();
        db.set_message_embedding(id_for("m3"), &[0.0, 0.0, 1.0])
            .unwrap();

        let filter = crate::rag::SearchFilter::default().with_limit(3);
        let results = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(db.search_messages("irrelevant", vec![0.0, 1.0, 0.0], filter))
            .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].content, "bravo");
    }

    #[test]
    fn test_vector_search_prefers_recent_with_equal_similarity() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        let now = Utc::now();
        let old_ts = (now - Duration::days(10)).timestamp();
        let new_ts = now.timestamp();

        db.save_message("old", "g1", "c1", "u1", "older", old_ts)
            .unwrap();
        db.save_message("new", "g1", "c1", "u1", "newer", new_ts)
            .unwrap();

        let id_for = |discord_id: &str| -> i64 {
            let conn = db.conn.lock().unwrap();
            conn.query_row(
                "SELECT id FROM messages WHERE discord_id = ?1",
                [discord_id],
                |row| row.get(0),
            )
            .unwrap()
        };

        db.set_message_embedding(id_for("old"), &[1.0, 0.0, 0.0])
            .unwrap();
        db.set_message_embedding(id_for("new"), &[1.0, 0.0, 0.0])
            .unwrap();

        let filter = crate::rag::SearchFilter::default().with_limit(2);
        let results = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(db.search_messages("query", vec![1.0, 0.0, 0.0], filter))
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].content, "newer");
    }

    #[test]
    fn test_vector_search_falls_back_to_keyword_when_no_embeddings_present() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        db.save_message("m1", "g1", "c1", "u1", "hello world", 1700000000)
            .unwrap();
        db.save_message("m2", "g1", "c1", "u1", "goodbye", 1700000001)
            .unwrap();

        let filter = crate::rag::SearchFilter::default().with_limit(5);
        let results = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(db.search_messages("hello", vec![1.0, 0.0, 0.0], filter))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "hello world");
    }

    #[test]
    fn test_hybrid_search_merges_keyword_results() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        db.save_message("m1", "g1", "c1", "u1", "alpha", 1700000000)
            .unwrap();
        db.save_message("m2", "g1", "c1", "u1", "hello world", 1700000001)
            .unwrap();

        let id_for = |discord_id: &str| -> i64 {
            let conn = db.conn.lock().unwrap();
            conn.query_row(
                "SELECT id FROM messages WHERE discord_id = ?1",
                [discord_id],
                |row| row.get(0),
            )
            .unwrap()
        };

        db.set_message_embedding(id_for("m1"), &[1.0, 0.0, 0.0])
            .unwrap();

        let filter = crate::rag::SearchFilter::default().with_limit(5);
        let results = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(db.search_messages("hello", vec![1.0, 0.0, 0.0], filter))
            .unwrap();

        assert!(results.iter().any(|r| r.content == "alpha"));
        assert!(results.iter().any(|r| r.content == "hello world"));
    }
}
