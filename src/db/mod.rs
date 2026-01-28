use rusqlite::{Connection, Result};
use std::sync::{Arc, Mutex};
use crate::config::Config;

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
        ";
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(sql)?;
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
        
        let mut sql = String::from("SELECT content, user_id, timestamp, channel_id FROM messages WHERE 1=1");
        
        // Basic keyword search if not vector
        if !query.is_empty() {
             sql.push_str(&format!(" AND content LIKE '%{}%'", query.replace("'", "''")));
        }
        
        if !filter.channels.is_empty() {
             sql.push_str(" AND channel_id IN (");
             sql.push_str(&filter.channels.iter().map(|c| format!("'{}'", c)).collect::<Vec<_>>().join(","));
             sql.push_str(")");
        }

        if let Some(from) = filter.from_date {
            sql.push_str(&format!(" AND timestamp >= '{}'", from.format("%Y-%m-%d %H:%M:%S")));
        }

        sql.push_str(" LIMIT ?");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([filter.limit], |row| {
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
            mcp_servers: Vec::new(),
            context_message_limit: 50,
            context_retention_hours: 24,
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
}
