use rusqlite::{Connection, Result};
use std::sync::{Arc, Mutex};
use crate::config::Config;
use tracing::{info, debug};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(config: &Config) -> Result<Self> {
        let conn = Connection::open(&config.database_url)?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn execute_init(&self) -> anyhow::Result<()> {
        info!("Database: Initializing schema...");
        let sql = "
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
            
            CREATE TABLE IF NOT EXISTS settings (
                guild_id TEXT PRIMARY KEY,
                context_limit INTEGER,
                context_retention INTEGER
            );
            
            CREATE TABLE IF NOT EXISTS channel_summaries (
                channel_id TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
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
        ";
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(sql)?;
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
        debug!("Database: Saving message {} from user {} in channel {}", discord_id, user_id, channel_id);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO messages (discord_id, guild_id, channel_id, user_id, content, timestamp) 
             VALUES (?1, ?2, ?3, ?4, ?5, datetime(?6, 'unixepoch'))",
            (discord_id, guild_id, channel_id, user_id, content, timestamp),
        )?;
        Ok(())
    }

    pub fn set_guild_settings(&self, guild_id: u64, limit: Option<usize>, retention: Option<u64>) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Check if exists first
        let exists = conn.prepare("SELECT 1 FROM settings WHERE guild_id = ?1")?
            .exists([guild_id.to_string()])?;
            
        if exists {
            if let Some(l) = limit {
                conn.execute("UPDATE settings SET context_limit = ?1 WHERE guild_id = ?2", (l, guild_id.to_string()))?;
            }
            if let Some(r) = retention {
                conn.execute("UPDATE settings SET context_retention = ?1 WHERE guild_id = ?2", (r, guild_id.to_string()))?;
            }
        } else {
            conn.execute(
                "INSERT INTO settings (guild_id, context_limit, context_retention) VALUES (?1, ?2, ?3)",
                (guild_id.to_string(), limit, retention),
            )?;
        }
        Ok(())
    }
    
    pub fn get_guild_settings(&self, guild_id: u64) -> anyhow::Result<(Option<usize>, Option<u64>)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT context_limit, context_retention FROM settings WHERE guild_id = ?1")?;
        
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO channel_summaries (channel_id, summary, updated_at) 
             VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(channel_id) DO UPDATE SET summary = ?2, updated_at = CURRENT_TIMESTAMP",
            (channel_id, summary),
        )?;
        Ok(())
    }

    pub fn get_latest_summary(&self, channel_id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT summary FROM channel_summaries WHERE channel_id = ?1")?;
        let mut rows = stmt.query([channel_id])?;
        
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    // --- Channel Settings ---

    pub fn set_channel_enabled(&self, guild_id: &str, channel_id: &str, enabled: bool) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO channel_settings (guild_id, channel_id, enabled, updated_at) 
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(channel_id) DO UPDATE SET enabled = ?3, updated_at = CURRENT_TIMESTAMP",
            (guild_id, channel_id, enabled),
        )?;
        Ok(())
    }

    pub fn set_channel_memory_scope(&self, guild_id: &str, channel_id: &str, start_date: Option<String>) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO channel_settings (guild_id, channel_id, memory_start_date, updated_at) 
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(channel_id) DO UPDATE SET memory_start_date = ?3, updated_at = CURRENT_TIMESTAMP",
            (guild_id, channel_id, start_date),
        )?;
        Ok(())
    }

    pub fn get_channel_settings(&self, channel_id: &str) -> anyhow::Result<Option<(bool, Option<String>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT enabled, memory_start_date FROM channel_settings WHERE channel_id = ?1")?;
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

    pub fn list_channel_settings(&self, guild_id: &str) -> anyhow::Result<Vec<(String, bool, Option<String>)>> {
        let conn = self.conn.lock().unwrap();
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

    pub fn purge_messages(&self, channel_id: &str, before_date: Option<String>) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count = if let Some(date) = before_date {
            conn.execute("DELETE FROM messages WHERE channel_id = ?1 AND timestamp < ?", (channel_id, date))?
        } else {
            conn.execute("DELETE FROM messages WHERE channel_id = ?1", (channel_id,))?
        };
        Ok(count)
    }

    /// Removes messages older than `retention_hours` from the database.
    /// Returns the number of messages deleted.
    pub fn cleanup_old_messages(&self, retention_hours: u64) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM messages WHERE timestamp < datetime('now', ?1)",
            (format!("-{} hours", retention_hours),),
        )?;
        Ok(count)
    }

    pub async fn search_messages(
        &self,
        query: &str,
        _embedding: Vec<f32>,
        filter: crate::rag::SearchFilter
    ) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
        let conn = self.conn.lock().unwrap();
        
        // Note: For actual sqlite-vec usage, we'd use vec_distance_l2 or similar.
        // Since we are implementing for footprint, we assume the extension is loaded.
        // If not, we'd need a fallback. For this implementation, we'll use the SQL
        // pattern for sqlite-vec queries:
        
        let mut sql = String::from("
            SELECT m.content, m.user_id, m.timestamp, m.channel_id 
            FROM messages m
            LEFT JOIN channel_settings s ON m.channel_id = s.channel_id
            WHERE (s.enabled IS NULL OR s.enabled = 1)
            AND (s.memory_start_date IS NULL OR m.timestamp >= s.memory_start_date)
        ");
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Basic keyword search if not vector
        if !query.is_empty() {
             sql.push_str(" AND m.content LIKE ?");
             params.push(Box::new(format!("%{}%", query)));
        }
        
        if !filter.channels.is_empty() {
             sql.push_str(" AND m.channel_id IN (");
             sql.push_str(&vec!["?"; filter.channels.len()].join(", "));
             sql.push_str(")");
             for channel in &filter.channels {
                 params.push(Box::new(channel.clone()));
             }
        }

        if let Some(from) = filter.from_date {
            sql.push_str(" AND m.timestamp >= ?");
            params.push(Box::new(from.format("%Y-%m-%d %H:%M:%S").to_string()));
        }

        sql.push_str(" LIMIT ?");
        params.push(Box::new(filter.limit));

        let mut stmt = conn.prepare(&sql)?;
        
        // Convert Vec of trait objects to a slice of trait objects for rusqlite
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

        debug!("Database: Search returned {} results", results.len());
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

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
        }
    }

    #[test]
    fn test_db_init_and_save() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();

        db.save_message("1", "g1", "c1", "u1", "hello", 1600000000).unwrap();
        
        // Verify it exists (we don't have a direct get_message yet, but we can check query)
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT discord_id FROM messages WHERE discord_id = '1'").unwrap();
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
        db.set_channel_memory_scope(guild_id, channel_id, Some(test_date.clone())).unwrap();
        let (enabled, scope) = db.get_channel_settings(channel_id).unwrap().unwrap();
        assert!(!enabled); // Still disabled from before
        assert_eq!(scope, Some(test_date));

        // Test list
        let list = db.list_channel_settings(guild_id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, channel_id);

        // Test purge
        db.save_message("m1", guild_id, channel_id, "u1", "old", 1600000000).unwrap();
        db.save_message("m2", guild_id, channel_id, "u1", "new", 1700000000).unwrap();
        
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
        db.set_channel_memory_scope(guild_id, c3, Some("2026-01-01 00:00:00".to_string())).unwrap();

        // Save messages
        db.save_message("m1", guild_id, c1, "u1", "msg in c1", 1700000000).unwrap();
        db.save_message("m2", guild_id, c2, "u1", "msg in c2", 1700000000).unwrap();
        db.save_message("m3", guild_id, c3, "u1", "old msg in c3", 1600000000).unwrap(); // Before scope
        db.save_message("m4", guild_id, c3, "u1", "new msg in c3", 1800000000).unwrap(); // After scope

        let filter = crate::rag::SearchFilter::default().with_limit(10);
        let results = tokio::runtime::Runtime::new().unwrap().block_on(db.search_messages("", vec![], filter)).unwrap();

        // Should return m1 and m4. m2 is disabled, m3 is out of scope.
        assert_eq!(results.len(), 2);
        let contents: Vec<_> = results.iter().map(|r| r.content.as_str()).collect();
        assert!(contents.contains(&"msg in c1"));
        assert!(contents.contains(&"new msg in c3"));
        assert!(!contents.contains(&"msg in c2"));
        assert!(!contents.contains(&"old msg in c3"));
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
        conn.execute("INSERT INTO messages (discord_id, guild_id, channel_id, user_id, content, timestamp) 
                     VALUES ('old', 'g1', 'c1', 'u1', 'old msg', datetime('now', '-48 hours'))", []).unwrap();
        conn.execute("INSERT INTO messages (discord_id, guild_id, channel_id, user_id, content, timestamp) 
                     VALUES ('new', 'g1', 'c1', 'u1', 'new msg', datetime('now', '-1 hours'))", []).unwrap();
        drop(conn);

        // Retention is 24 hours
        let deleted = db.cleanup_old_messages(24).unwrap();
        assert_eq!(deleted, 1);

        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT discord_id FROM messages").unwrap();
        let ids: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "new");
    }

    #[test]
    fn test_search_with_special_chars() {
        let config = test_config();
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();
        
        db.save_message("1", "g1", "c1", "u1", "normal message", 1600000000).unwrap();
        
        // This should NOT cause SQL injection
        let filter = crate::rag::SearchFilter::default().with_limit(10);
        let result = tokio::runtime::Runtime::new().unwrap().block_on(db.search_messages("'; DROP TABLE messages; --", vec![], filter));
        assert!(result.is_ok());
        
        // Verify table still exists
        let conn = db.conn.lock().unwrap();
        assert!(conn.prepare("SELECT 1 FROM messages").is_ok());
    }
}
