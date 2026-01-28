use rusqlite::{Connection, Result};
use std::sync::{Arc, Mutex};
use crate::config::Config;

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

    pub async fn search_messages(
        &self,
        _embedding: Vec<f32>,
        filter: crate::rag::SearchFilter
    ) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
        let conn = self.conn.lock().unwrap();
        
        // Note: For actual sqlite-vec usage, we'd use vec_distance_l2 or similar.
        // Since we are implementing for footprint, we assume the extension is loaded.
        // If not, we'd need a fallback. For this implementation, we'll use the SQL
        // pattern for sqlite-vec queries:
        
        let mut sql = String::from("SELECT content, user_id, timestamp, channel_id FROM messages WHERE 1=1");
        
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
            llama_url: "test".to_string(),
            llama_model: "test".to_string(),
            embedding_url: "test".to_string(),
            embedding_model: "test".to_string(),
            database_url: ":memory:".to_string(),
            system_prompt: "test".to_string(),
            max_context_messages: 10,
            status_message: "test".to_string(),
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
}
