use lru::LruCache;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;

/// Thread-safe message cache with LRU eviction and per-channel indexing
pub struct MessageCache {
    /// Primary message storage with LRU eviction
    cache: Arc<Mutex<LruCache<String, Message>>>,
    /// Per-channel message IDs (ordered by insertion, newest last)
    channel_index: Arc<Mutex<HashMap<ChannelId, VecDeque<String>>>>,
    capacity: usize,
}

impl MessageCache {
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(cap))),
            channel_index: Arc::new(Mutex::new(HashMap::new())),
            capacity,
        }
    }

    pub fn insert(&self, message: Message) {
        let message_id = message.id.to_string();
        let channel_id = message.channel_id;
        
        // Insert into main cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(message_id.clone(), message);
        }
        
        // Update channel index
        {
            let mut index = self.channel_index.lock().unwrap();
            let channel_msgs = index.entry(channel_id).or_insert_with(VecDeque::new);
            channel_msgs.push_back(message_id);
            
            // Keep channel index bounded (2x capacity to account for multi-channel spread)
            while channel_msgs.len() > self.capacity * 2 {
                channel_msgs.pop_front();
            }
        }
    }

    pub fn get(&self, message_id: &str) -> Option<Message> {
        let mut cache = self.cache.lock().unwrap();
        cache.get(message_id).cloned()
    }
    
    /// Retrieve recent messages from a specific channel, ordered oldest to newest
    pub fn get_channel_history(&self, channel_id: ChannelId, limit: usize) -> Vec<Message> {
        let index = self.channel_index.lock().unwrap();
        let cache = self.cache.lock().unwrap();
        
        let Some(channel_msgs) = index.get(&channel_id) else {
            return Vec::new();
        };
        
        // Take the last `limit` message IDs and resolve them
        channel_msgs
            .iter()
            .rev()
            .take(limit)
            .filter_map(|msg_id| cache.peek(msg_id).cloned())
            .collect::<Vec<_>>()
            .into_iter()
            .rev() // Restore oldest-first order
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenity::model::id::MessageId;
    use serenity::model::channel::Message;
    use serenity::model::user::User;
    use serenity::model::id::UserId;

    fn mock_message(id: u64, channel_id: u64) -> Message {
        let mut msg = Message::default();
        msg.id = MessageId::new(id);
        msg.channel_id = ChannelId::new(channel_id);
        msg.author = User::default();
        msg.author.id = UserId::new(1);
        msg.content = format!("Message {}", id);
        msg
    }

    #[test]
    fn test_cache_lru() {
        let cache = MessageCache::new(2);
        
        let m1 = mock_message(1, 100);
        let m2 = mock_message(2, 100);
        let m3 = mock_message(3, 100);

        cache.insert(m1.clone());
        cache.insert(m2.clone());
        
        assert!(cache.get("1").is_some());
        assert!(cache.get("2").is_some());

        cache.insert(m3.clone());
        assert!(cache.get("3").is_some());
        
        // Check that it stays within capacity (2 messages max)
        let mut count = 0;
        if cache.get("1").is_some() { count += 1; }
        if cache.get("2").is_some() { count += 1; }
        if cache.get("3").is_some() { count += 1; }
        
        assert_eq!(count, 2);
    }

    #[test]
    fn test_cache_channel_history() {
        let cache = MessageCache::new(100);
        
        // Insert messages to two channels
        cache.insert(mock_message(1, 100));
        cache.insert(mock_message(2, 100));
        cache.insert(mock_message(3, 200)); // Different channel
        cache.insert(mock_message(4, 100));
        cache.insert(mock_message(5, 100));
        
        // Get history for channel 100
        let history = cache.get_channel_history(ChannelId::new(100), 10);
        assert_eq!(history.len(), 4);
        
        // Verify order: oldest to newest
        assert_eq!(history[0].content, "Message 1");
        assert_eq!(history[3].content, "Message 5");
        
        // Test limit
        let limited = cache.get_channel_history(ChannelId::new(100), 2);
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].content, "Message 4");
        assert_eq!(limited[1].content, "Message 5");
        
        // Get history for channel 200
        let history_200 = cache.get_channel_history(ChannelId::new(200), 10);
        assert_eq!(history_200.len(), 1);
        
        // Non-existent channel
        let empty = cache.get_channel_history(ChannelId::new(999), 10);
        assert!(empty.is_empty());
    }
}
