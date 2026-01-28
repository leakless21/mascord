use chrono::{DateTime, Utc};

#[derive(Default)]
pub struct SearchFilter {
    pub channels: Vec<String>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub limit: usize,
}

impl SearchFilter {
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_from_date(mut self, from: DateTime<Utc>) -> Self {
        self.from_date = Some(from);
        self
    }

    pub fn with_channel(mut self, channel_id: String) -> Self {
        self.channels.push(channel_id);
        self
    }
}

pub struct MessageResult {
    pub content: String,
    pub user_id: String,
    pub timestamp: String,
    pub channel_id: String,
}
