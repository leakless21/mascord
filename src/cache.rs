use chrono::{Duration, Utc};
use lru::LruCache;
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tracing::warn;

/// Thread-safe message cache with LRU eviction and per-channel indexing
#[derive(Clone)]
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

    fn lock_cache(&self) -> std::sync::MutexGuard<'_, LruCache<String, Message>> {
        match self.cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Message cache lock poisoned; recovering");
                poisoned.into_inner()
            }
        }
    }

    fn lock_index(&self) -> std::sync::MutexGuard<'_, HashMap<ChannelId, VecDeque<String>>> {
        match self.channel_index.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Message cache index lock poisoned; recovering");
                poisoned.into_inner()
            }
        }
    }

    pub fn insert(&self, message: Message) {
        let message_id = message.id.to_string();
        let channel_id = message.channel_id;

        // Insert into main cache
        {
            let mut cache = self.lock_cache();
            cache.put(message_id.clone(), message);
        }

        // Update channel index
        {
            let mut index = self.lock_index();
            let channel_msgs = index.entry(channel_id).or_default();
            channel_msgs.push_back(message_id);

            // Keep channel index bounded (2x capacity to account for multi-channel spread)
            while channel_msgs.len() > self.capacity * 2 {
                channel_msgs.pop_front();
            }
        }
    }

    pub fn get(&self, message_id: &str) -> Option<Message> {
        let mut cache = self.lock_cache();
        cache.get(message_id).cloned()
    }

    /// Retrieve recent messages from a specific channel, ordered oldest to newest
    pub fn get_channel_history(&self, channel_id: ChannelId, limit: usize) -> Vec<Message> {
        let index = self.lock_index();
        let cache = self.lock_cache();

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

    /// Remove cached messages older than the given retention window.
    /// Returns the number of messages removed.
    pub fn cleanup_old_messages(&self, retention_hours: u64) -> usize {
        if retention_hours == 0 {
            return 0;
        }

        let cutoff = Utc::now() - Duration::hours(retention_hours as i64);
        let cutoff_unix = cutoff.timestamp();

        let mut cache = self.lock_cache();
        let mut expired = Vec::new();
        for (key, msg) in cache.iter() {
            if msg.timestamp.unix_timestamp() < cutoff_unix {
                expired.push(key.clone());
            }
        }

        for key in &expired {
            cache.pop(key);
        }
        drop(cache);

        if expired.is_empty() {
            return 0;
        }

        let expired_set: HashSet<String> = expired.into_iter().collect();
        let mut index = self.lock_index();
        for queue in index.values_mut() {
            queue.retain(|id| !expired_set.contains(id));
        }

        expired_set.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenity::model::channel::Message;
    use serenity::model::id::MessageId;
    use serenity::model::id::UserId;
    use serenity::model::timestamp::Timestamp;
    use serenity::model::user::User;

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
        if cache.get("1").is_some() {
            count += 1;
        }
        if cache.get("2").is_some() {
            count += 1;
        }
        if cache.get("3").is_some() {
            count += 1;
        }

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

    #[test]
    fn test_cache_cleanup_old_messages() {
        let cache = MessageCache::new(10);

        let mut old_msg = mock_message(1, 100);
        old_msg.timestamp = Timestamp::from_unix_timestamp(1).unwrap();
        cache.insert(old_msg);

        let mut new_msg = mock_message(2, 100);
        new_msg.timestamp = Timestamp::from_unix_timestamp(Utc::now().timestamp()).unwrap();
        cache.insert(new_msg);

        let removed = cache.cleanup_old_messages(1);
        assert_eq!(removed, 1);

        let history = cache.get_channel_history(ChannelId::new(100), 10);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, MessageId::new(2));
    }
}
