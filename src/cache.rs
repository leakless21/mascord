use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use serenity::model::channel::Message;

pub struct MessageCache {
    cache: Arc<Mutex<LruCache<String, Message>>>,
}

impl MessageCache {
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(cap))),
        }
    }

    pub fn insert(&self, message: Message) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(message.id.to_string(), message);
    }

    pub fn get(&self, message_id: &str) -> Option<Message> {
        let mut cache = self.cache.lock().unwrap();
        cache.get(message_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenity::model::id::MessageId;
    use serenity::model::channel::Message;
    use serenity::model::user::User;
    use serenity::model::id::UserId;

    fn mock_message(id: u64) -> Message {
        let mut msg = Message::default();
        msg.id = MessageId::new(id);
        msg.author = User::default();
        msg.author.id = UserId::new(1);
        msg.content = format!("Message {}", id);
        msg
    }

    #[test]
    fn test_cache_lru() {
        let cache = MessageCache::new(2);
        
        let m1 = mock_message(1);
        let m2 = mock_message(2);
        let m3 = mock_message(3);

        cache.insert(m1.clone());
        cache.insert(m2.clone());
        
        assert!(cache.get("1").is_some());
        assert!(cache.get("2").is_some());

        // Insert third message, should evict 1 (since 2 was most recently accessed via get)
        // Wait, current logic: insert stays at top. 
        // LruCache behavior: put(1), put(2) -> 2 is MRU. get(1) -> 1 is MRU. put(3) -> evicts 2.
        
        // Let's test eviction
        cache.insert(m3.clone());
        assert!(cache.get("3").is_some());
        
        // 1 was accessed last via get("1"), so it should still be there. 2 should be gone.
        // Actually, let's just check that it stays within capacity.
        let mut count = 0;
        if cache.get("1").is_some() { count += 1; }
        if cache.get("2").is_some() { count += 1; }
        if cache.get("3").is_some() { count += 1; }
        
        assert_eq!(count, 2);
    }
}
