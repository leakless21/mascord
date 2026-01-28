//! Conversation context management for persistent LLM memory
//! 
//! Provides per-channel context retrieval for injecting recent message history
//! into LLM conversations.

use async_openai::types::{
    ChatCompletionRequestMessage,
    ChatCompletionRequestUserMessageArgs,
    ChatCompletionRequestAssistantMessageArgs,
};
use chrono::{Utc, Duration};
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;

use crate::cache::MessageCache;
use crate::config::Config;
use crate::db::Database;

/// Formats cached messages into LLM-compatible context messages
pub struct ConversationContext;

impl ConversationContext {
    /// Retrieves recent channel messages and formats them for LLM context
    /// 
    /// Messages are filtered by:
    /// - Channel ID
    /// - Retention period (config.context_retention_hours)
    /// - Limit (config.context_message_limit)
    /// 
    /// Returns messages oldest-first, formatted as user/assistant messages
    pub fn get_context_for_channel(
        cache: &MessageCache,
        db: &Database,
        config: &Config,
        channel_id: ChannelId,
        guild_id: Option<u64>,
        bot_id: Option<u64>,
    ) -> Vec<ChatCompletionRequestMessage> {
        // Resolve settings: Check DB -> Fallback to Config
        let (limit, retention) = if let Some(gid) = guild_id {
            db.get_guild_settings(gid)
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        
        let limit = limit.unwrap_or(config.context_message_limit);
        let retention = retention.unwrap_or(config.context_retention_hours);

        let cutoff_unix = (Utc::now() - Duration::hours(retention as i64)).timestamp();
        
        let mut messages = Vec::new();

        // 1. Inject Working Memory (Latest Summary) if available
        if let Ok(Some(summary)) = db.get_latest_summary(&channel_id.to_string()) {
            use async_openai::types::ChatCompletionRequestSystemMessageArgs;
            if let Ok(msg) = ChatCompletionRequestSystemMessageArgs::default()
                .content(format!("Earlier conversation summary for this channel:\n{}", summary))
                .build()
            {
                messages.push(msg.into());
            }
        }

        // 2. Fetch Short-Term context (verbatim messages)
        let entries = cache.get_channel_history(channel_id, limit);
        
        let mut short_term_messages: Vec<ChatCompletionRequestMessage> = entries
            .into_iter()
            .filter(|msg| {
                // Filter by retention period using unix timestamps
                msg.timestamp.unix_timestamp() > cutoff_unix
            })
            .filter_map(|msg| {
                Self::format_message(&msg, bot_id)
            })
            .collect();
            
        messages.append(&mut short_term_messages);
        messages
    }
    
    /// Formats a Discord message into an LLM message
    fn format_message(msg: &Message, bot_id: Option<u64>) -> Option<ChatCompletionRequestMessage> {
        // Skip empty messages
        if msg.content.trim().is_empty() {
            return None;
        }
        
        let is_bot = bot_id.map_or(false, |id| msg.author.id.get() == id);
        
        if is_bot {
            // Bot's own messages become assistant messages
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(msg.content.clone())
                .build()
                .ok()
                .map(|m| m.into())
        } else {
            // Other users' messages become user messages with attribution
            let formatted = format!("[{}]: {}", msg.author.name, msg.content);
            ChatCompletionRequestUserMessageArgs::default()
                .content(formatted)
                .build()
                .ok()
                .map(|m| m.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenity::model::id::MessageId;
    use serenity::model::user::User;
    use serenity::model::id::UserId;
    use serenity::model::timestamp::Timestamp;
    
    fn mock_config() -> Config {
        Config {
            discord_token: "test".to_string(),
            application_id: 0,
            owner_id: None,
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
            context_message_limit: 5,
            context_retention_hours: 24,
        }
    }
    
    fn mock_message(id: u64, channel_id: u64, user_id: u64, content: &str, username: &str) -> Message {
        let mut msg = Message::default();
        msg.id = MessageId::new(id);
        msg.channel_id = ChannelId::new(channel_id);
        msg.author = User::default();
        msg.author.id = UserId::new(user_id);
        msg.author.name = username.to_string();
        msg.content = content.to_string();
        msg.timestamp = Timestamp::now();
        msg
    }
    
    #[test]
    fn test_context_retrieval() {
        let cache = MessageCache::new(100);
        let config = mock_config();
        
        // Setup in-memory DB
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();
        
        cache.insert(mock_message(1, 100, 1, "Hello everyone", "Alice"));
        cache.insert(mock_message(2, 100, 2, "Hi Alice!", "Bob"));
        cache.insert(mock_message(3, 100, 999, "Hello, how can I help?", "Mascord")); // Bot
        cache.insert(mock_message(4, 100, 1, "What's the weather?", "Alice"));
        
        let context = ConversationContext::get_context_for_channel(
            &cache,
            &db,
            &config,
            ChannelId::new(100),
            Some(123),
            Some(999), // Bot ID
        );
        
        assert_eq!(context.len(), 4);
    }
    
    #[test]
    fn test_context_limit() {
        let cache = MessageCache::new(100);
        let config = mock_config(); // limit = 5
        let db = Database::new(&config).unwrap();
        db.execute_init().unwrap();
        
        for i in 1..=10 {
            cache.insert(mock_message(i, 100, 1, &format!("Message {}", i), "User"));
        }
        
        let context = ConversationContext::get_context_for_channel(
            &cache,
            &db,
            &config,
            ChannelId::new(100),
            Some(123),
            None,
        );
        
        // Should only get 5 messages (the most recent ones)
        assert_eq!(context.len(), 5);
    }
}
