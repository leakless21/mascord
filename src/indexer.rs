use crate::db::Database;
use crate::llm::LlmClient;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{debug, error, info};

pub struct EmbeddingIndexer {
    db: Database,
    llm: Arc<LlmClient>,
    batch_size: usize,
    interval: Duration,
}

impl EmbeddingIndexer {
    pub fn new(db: Database, llm: Arc<LlmClient>, batch_size: usize, interval: Duration) -> Self {
        Self {
            db,
            llm,
            batch_size,
            interval,
        }
    }

    pub async fn run(self) {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            ticker.tick().await;
            match self.process_batch().await {
                Ok(0) => debug!("Embedding indexer: no messages to index"),
                Ok(n) => info!("Embedding indexer: indexed {} messages", n),
                Err(e) => error!("Embedding indexer error: {}", e),
            }
        }
    }

    async fn process_batch(&self) -> anyhow::Result<usize> {
        let batch_size = self.batch_size;
        let db = self.db.clone();
        let pending =
            tokio::task::spawn_blocking(move || db.get_messages_missing_embeddings(batch_size))
                .await??;

        let mut indexed = 0usize;
        for (id, content) in pending {
            // Skip very short messages to reduce embedding noise/cost.
            if content.trim().len() < 3 {
                let db = self.db.clone();
                tokio::task::spawn_blocking(move || db.mark_message_indexed(id)).await??;
                continue;
            }

            match self.llm.get_embeddings(&content).await {
                Ok(embedding) => {
                    let db = self.db.clone();
                    tokio::task::spawn_blocking(move || db.set_message_embedding(id, &embedding))
                        .await??;
                    indexed += 1;
                }
                Err(e) => {
                    debug!("Embedding indexer: failed to embed message {}: {}", id, e);
                }
            }
        }

        Ok(indexed)
    }
}
