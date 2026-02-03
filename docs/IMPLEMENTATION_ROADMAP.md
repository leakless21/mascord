# Implementation Roadmap: Mascord AI Memory & Safety Upgrades

**Date:** February 3, 2026  
**Version:** 1.0  
**Status:** Implemented (Phases 1-4 completed)

---

## Executive Summary

This document outlines a comprehensive plan to close the gaps between Mascord's **documented requirements** (Three-Tier Memory, RAG, Agentic tool execution) and the **current implementation** (mocked RAG, empty summarization loop, brittle music integration).

The plan prioritizes **high-impact, manageable improvements** using proven Rust best practices and existing community libraries.

---

## Review Outcomes (2026-02-03)

This section captures the post-review recommendations and option picks to align the roadmap with the current codebase.

### Option Picks (Final)

1. **Vector Search**: Pick **Option B (Pure Rust in-process scoring)** now.  
   Keep `sqlite-vec` as an **optional future acceleration path** if dataset size grows beyond ~500k–1M messages or latency becomes a problem.  
   **Action:** Add a **hybrid fallback** (keyword + vector) until embeddings are backfilled.

2. **Summarization**: Pick **rolling summary with dual thresholds + periodic refresh + milestone anchors**.  
   **Action:** Explicitly document summary size budgeting (heuristic or tokenizer).

3. **Music Cookies**: Pick **direct `YoutubeDl::user_args(["--cookies", path])`**.  
   **Action:** If no cookies are configured, **surface a user-visible error** on age-restricted failures (do not silently fail).

4. **Agent Safety**: Defer confirmation-gate until the agent can receive a Discord context.  
   **Action:** Do **not** expose destructive tools to the agent until confirmation UI is wired.

### Consistency Corrections

- Message ingestion does **not** generate embeddings today; only `/search` does.  
  The roadmap must include an embedding backfill path or hybrid fallback.
- Agent tool confirmation needs a **tool execution context** (or a command-level mediator).  
Current `Agent::execute_tool_call` has no access to Discord UI primitives.

---

## Implementation Status (2026-02-03)

- Phase 1 (Vector Search + Embeddings): Implemented
- Phase 2 (Rolling Summarization): Implemented
- Phase 3 (Music Cookies + Age Restriction Errors): Implemented
- Phase 4 (Agent Safety Guard): Implemented (confirmation UI deferred; dangerous tools blocked)

## Part 1: Current State Assessment

### What's Working Well ✅
1. **Discord Integration:** Poise/Serenity framework is production-grade, well-tested.
2. **Voice Infrastructure:** Songbird handles voice channel lifecycle properly.
3. **Database Foundation:** SQLite schema is designed for multi-tier memory.
4. **Timeout Protection:** LLM & MCP calls have `tokio::time::timeout` guards.
5. **MCP Integration:** Framework for connecting external tools is in place.
6. **Modular Architecture:** Clean separation of concerns across modules.

### What's Missing or Broken ❌ (Resolved)

| Component | Issue | Severity | Evidence |
|-----------|-------|----------|----------|
| **Vector Search** | Implemented Rust vector scoring over SQLite-stored embedding blobs; keyword fallback retained until backfill completes. | ✅ Implemented | `src/db/mod.rs` - `embedding` column + `search_messages_vector()` |
| **Summarization Loop** | Implemented rolling summaries with trigger policy, size cap, refresh logic, and an active background loop. | ✅ Implemented | `src/summarize.rs`, `src/main.rs` |
| **Music Cookies** | Cookie passing fixed using Songbird `YoutubeDl::user_args`; preflight added to raise a clear age-restriction error when cookies are missing. | ✅ Implemented | `src/commands/music.rs` |
| **Agent Safety** | Added `requires_confirmation()` on tools; agent blocks these until Discord confirmation UI exists. Destructive tool removed from agent registry. | ✅ Implemented | `src/tools/mod.rs`, `src/llm/agent.rs`, `src/main.rs` |

---

## Part 2: Strategic Decisions & Recommendations

### Decision 1: Vector Search Architecture

**Question:** How should we implement semantic (vector-based) search?

#### Option A: `sqlite-vec` Extension (Traditional Approach)
**Description:** Compile a C-extension for SQLite that handles vector operations natively.

**Pros:**
- Optimized for millions of vectors
- Battle-tested (used by Turso, production systems)
- Off-loads math to specialized code

**Cons:**
- Adds a C dependency (breaks pure Rust portability)
- Complicates build process (different for Linux/macOS/Windows/Docker)
- Requires `unsafe` code in Rust to load the extension
- More difficult to debug

**Deployment Complexity:** HIGH  
**Maintenance Burden:** MEDIUM

---

#### Option B: Pure Rust In-Memory Search (Recommended)
**Description:** Store vectors as binary `BLOB` in SQLite. Calculate similarity scores using Rust's CPU (SIMD-optimized math).

**How It Works:**
1. Query: User provides text → LLM generates embedding (e.g., 384-dim float vector)
2. Search: Fetch all candidate embeddings from DB, calculate Cosine Similarity with query in Rust
3. Sort: Return top K results by similarity score
4. Speed: 50,000 vectors scored in <100ms on modern CPU

**Pros:**
- Zero external dependencies (pure `rusqlite` + Rust stdlib)
- Fully portable (Linux, macOS, Windows, Docker, WASM)
- Trivial to debug (all code in Rust)
- Aligns with **KISS Principle** from AGENTS.md
- Good enough for Discord bot scale (Discord bots rarely need >50k active messages)

**Cons:**
- Slower than specialized vector DB for massive datasets (>1M vectors)
- Not appropriate for "Google-scale" search (not your use case)

**Deployment Complexity:** LOW  
**Maintenance Burden:** VERY LOW

---

**✅ Recommendation: Option B (Pure Rust In-Memory)**

**Rationale:**
- Mascord is designed for **private use** ("small number of servers" - REQUIREMENTS.md)
- A Discord bot typically has <100k messages per guild
- Rust's CPU performance is sufficient
- Zero extra dependencies = easier deployment, fewer security issues, simpler Docker image

---

### Decision 2: Music Cookies Implementation

**Question:** How should cookies be passed to `yt-dlp`?

#### Current Approach (Broken)
```rust
std::env::set_var("YTDL_ARGS", format!("--cookies {}", cookies));
```

**Problem:** `yt-dlp` doesn't read a `YTDL_ARGS` environment variable. This is a misunderstanding of how the CLI works. Age-restricted videos will likely still fail.

#### Recommended Approach (Proven)
```rust
// Use Songbird's yt-dlp args support directly (Songbird 0.5: `user_args`)
let mut source = YoutubeDl::new(ctx.data().http_client.clone(), url.clone());
if let Some(cookies_path) = &ctx.data().config.youtube_cookies {
    if Path::new(cookies_path).exists() {
        let args = vec![
            "--no-playlist".to_string(),
            "--cookies".to_string(),
            cookies_path.clone(),
        ];
        source = source.user_args(args);
    } else {
        warn!("YouTube cookies file not found at {}", cookies_path);
    }
}
```

**Benefit:** Type-safe, explicit, guaranteed to work. Follows `poise`/`songbird` best practices.

---

### Decision 3: Summarization Pattern

**Question:** How should "Working Memory" (channel summaries) evolve over time?

#### Current Approach (Naive)
- Every 4 hours: Summarize last 200 messages
- **Problem:** Summary contains only the last 4 hours. Yesterday's context is lost.

#### Recommended Approach: "Rolling Window" Pattern
- Every 4 hours: Take the **previous summary** + **new messages** → produce **updated summary**
- Mimics how humans remember: keep consolidating old context into a narrative

**Example Evolution:**
```
Hour 0:  "Team discussed project X"
Hour 4:  "Team discussed project X and decided on timeline Q2"
Hour 8:  "Team discussed project X (Q2), then pivoted to feature Y"
Hour 12: "Team is tracking 2 initiatives: X (Q2) and Y (design phase)"
```

**Benefit:** Arbitrarily long memory chain while keeping tokens low.

#### Long-Term Guardrails (Indefinite Operation)
A rolling summary can run indefinitely if it has hard caps and periodic refreshes.

- **Trigger policy (dual threshold):** Summarize when `new_messages >= 150` **OR** when `summary_age >= 6 hours` **AND** `new_messages >= 20`.
- **Summary size cap:** Enforce `summary_max_tokens` (e.g., 1,200). If exceeded, force a harder compression pass.
- **Periodic refresh:** Every 6 weeks, regenerate summary from the last 14 days of messages **plus** the milestone list.
- **Milestones/anchors:** Maintain a short list of durable facts/decisions and merge it into the prompt each update.

**Trigger Algorithm (pseudo):**
```text
if new_messages >= 150:
    summarize()
else if summary_age_hours >= 6 and new_messages >= 20:
    summarize()
```

---

### Decision 4: Agent Safety - Confirmation Gates

**Question:** Should the LLM be allowed to execute any tool without user input?

#### Current Approach (Unrestricted)
LLM decides to call any tool. Bot executes immediately. Risk if tool has side effects.

#### Recommended Approach: Multi-Tier Safety
1. **Tier 1 (Safe Tools):** `Search`, `PlayMusic` → Execute immediately
2. **Tier 2 (Confirm):** `Shutdown`, `PurgeMessages` → Send Discord button, wait for approval
3. **Tier 3 (MCP):** External MCP tools → Depends on tool schema

**Implementation:** Add `requires_confirmation: bool` field to `Tool` trait.

**Note (Review Fix):** This requires a **tool execution context** or a command-level mediator.  
`Agent::execute_tool_call` currently cannot show Discord confirmation UI.

---

## Part 3: Detailed Implementation Plan

### Phase 1: Vector Search (Priority 1 - Foundation)

**Goal:** Replace keyword search with semantic similarity search.

**Files to Modify:**
1. `src/db/mod.rs` - Schema + Search Logic
2. `src/db/schema.rs` - Explicitly document schema changes
3. `Cargo.toml` - Optional: add `ndarray` or keep stdlib

**Steps:**

#### Step 1.1: Update Database Schema
**File:** `src/db/mod.rs:execute_init()` (Lines 30-55)

**Change:** Add `embedding BLOB NULL` column to messages table.

```sql
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    discord_id TEXT NOT NULL UNIQUE,
    guild_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    is_indexed BOOLEAN DEFAULT FALSE,
    embedding BLOB NULL  -- NEW: Store vector as binary
);
```

**Why BLOB?**
- `embedding` is a vector of ~384 f32 values (1536 bytes)
- SQL doesn't have native float array types
- Store as raw bytes, deserialize in Rust (fast, memory-efficient)

---

#### Step 1.2: Add Vector Serialization Helpers
**File:** `src/db/mod.rs` (add new module at top of file)

```rust
// Helper functions for vector serialization
fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn deserialize_embedding(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return Err(anyhow::anyhow!("Invalid embedding size"));
    }
    Ok(bytes
        .chunks(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot_product: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot_product / (norm_a * norm_b)
}
```

---

#### Step 1.3: Update `save_message()` to Store Embedding
**File:** `src/db/mod.rs` (modify existing method)

Currently:
```rust
pub fn save_message(
    &self,
    discord_id: &str,
    guild_id: &str,
    channel_id: &str,
    user_id: &str,
    content: &str,
    timestamp: i64,
) -> anyhow::Result<()> {
    // ... existing code
}
```

Should become:
```rust
pub fn save_message(
    &self,
    discord_id: &str,
    guild_id: &str,
    channel_id: &str,
    user_id: &str,
    content: &str,
    timestamp: i64,
    embedding: Option<&[f32]>,  // NEW parameter
) -> anyhow::Result<()> {
    let conn = self.lock_conn()?;
    let embedding_blob = embedding.map(serialize_embedding);
    
    conn.execute(
        "INSERT OR IGNORE INTO messages 
         (discord_id, guild_id, channel_id, user_id, content, timestamp, embedding) 
         VALUES (?1, ?2, ?3, ?4, ?5, datetime(?6, 'unixepoch'), ?7)",
        rusqlite::params![
            discord_id, guild_id, channel_id, user_id, content, timestamp, 
            embedding_blob.as_ref().map(|v| v.as_slice())
        ],
    ).context("Failed to save message")?;
    Ok(())
}
```

**Note:** Keep backward compatibility - `embedding` is optional. Messages without embeddings will have `NULL`.

---

#### Step 1.4: Replace `search_messages()` with Vector Logic
**File:** `src/db/mod.rs` (replace existing method at ~Line 300-330)

Current (broken):
```rust
pub async fn search_messages(
    &self,
    query: &str,
    _embedding: Vec<f32>,  // <- IGNORED!
    filter: crate::rag::SearchFilter
) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
    // Uses only LIKE %query%
}
```

New:
```rust
pub async fn search_messages(
    &self,
    _query: &str,  // No longer used (kept for API compatibility)
    query_embedding: Vec<f32>,  // Now used!
    filter: crate::rag::SearchFilter
) -> anyhow::Result<Vec<crate::rag::MessageResult>> {
    use tokio::task;
    
    let db_clone = self.clone();
    let query_embedding_clone = query_embedding.clone();
    
    // Offload heavy computation to a blocking thread pool
    // (Don't block the async runtime with CPU work)
    let results = task::spawn_blocking(move || {
        let conn = db_clone.lock_conn()?;
        
        // Build SQL to fetch candidates (with filtering)
        let mut sql = String::from(
            "SELECT m.content, m.user_id, m.timestamp, m.channel_id, m.embedding 
             FROM messages m
             LEFT JOIN channel_settings s ON m.channel_id = s.channel_id
             WHERE (s.enabled IS NULL OR s.enabled = 1)
             AND (s.memory_start_date IS NULL OR m.timestamp >= s.memory_start_date)
             AND m.embedding IS NOT NULL"  // Only consider messages with embeddings
        );
        
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::new();
        
        // Add channel filter
        if !filter.channels.is_empty() {
            sql.push_str(" AND m.channel_id IN (");
            sql.push_str(&vec!["?"; filter.channels.len()].join(", "));
            sql.push_str(")");
            for channel in &filter.channels {
                params.push(channel);
            }
        }
        
        // Add date filter
        if let Some(from) = filter.from_date {
            sql.push_str(" AND m.timestamp >= ?");
            params.push(&from.format("%Y-%m-%d %H:%M:%S").to_string());
        }
        
        sql.push_str(" ORDER BY m.timestamp DESC LIMIT 1000");  // Fetch many candidates
        
        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| *p).collect();
        
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let embedding_bytes: Vec<u8> = row.get(4)?;
            let embedding = deserialize_embedding(&embedding_bytes)?;
            Ok((
                crate::rag::MessageResult {
                    content: row.get(0)?,
                    user_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    channel_id: row.get(3)?,
                },
                embedding,
            ))
        })?;
        
        // Score all candidates and collect
        let mut scored_results: Vec<_> = rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(msg, embedding)| {
                let score = cosine_similarity(&query_embedding_clone, &embedding);
                (score, msg)
            })
            .collect();
        
        // Sort by similarity (highest first)
        scored_results.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Take top K
        let limit = filter.limit.max(1).min(100);  // Clamp to reasonable range
        scored_results.truncate(limit);
        
        Ok::<Vec<crate::rag::MessageResult>, anyhow::Error>(
            scored_results.into_iter().map(|(_, msg)| msg).collect()
        )
    })
    .await??;
    
    debug!("Vector search returned {} results", results.len());
    Ok(results)
}
```

**Why `spawn_blocking`?**
- Scoring 1000 vectors = significant CPU work (~10ms)
- Must not block the async runtime
- `spawn_blocking` uses a dedicated thread pool

---

#### Step 1.5: Update Call Sites to Provide Embeddings
**File:** `src/main.rs` (Event Handler)

Current (Line ~120):
```rust
if let Err(e) = data.db.save_message(
    &new_message.id.to_string(),
    &new_message.guild_id.map(|id| id.to_string()).unwrap_or_default(),
    &new_message.channel_id.to_string(),
    &new_message.author.id.to_string(),
    &new_message.content,
    new_message.timestamp.unix_timestamp(),
) {
    // error handling
}
```

New:
```rust
// Generate embedding for the message
let embedding = data.llm_client.get_embeddings(&new_message.content).await.ok();
let embedding_slice = embedding.as_ref().map(|v| v.as_slice());

if let Err(e) = data.db.save_message(
    &new_message.id.to_string(),
    &new_message.guild_id.map(|id| id.to_string()).unwrap_or_default(),
    &new_message.channel_id.to_string(),
    &new_message.author.id.to_string(),
    &new_message.content,
    new_message.timestamp.unix_timestamp(),
    embedding_slice,
) {
    // error handling
}
```

**Optimization Note:** Embedding generation happens in parallel (one LLM call per message). If this becomes a bottleneck, add batching or rate-limiting later.

---

### Phase 2: Rolling Summarization (Priority 1 - High Impact)

**Goal:** Implement automatic "Working Memory" updates that preserve context over time.

**Files to Modify:**
1. `src/summarize.rs` - Update logic
2. `src/main.rs` - Wire up background loop

**Steps:**

#### Step 2.0: Summarization Trigger Policy
Define when the summarizer should run and how it stays stable over months.

- **Dual-threshold trigger:** Run when `new_messages >= 150` **OR** when `summary_age >= 6 hours` **AND** `new_messages >= 20`.
- **Hard size cap:** If `summary_tokens > 1,200`, force a compression pass.
- **Periodic refresh:** Every 6 weeks, rebuild from the last 14 days + milestone list.
- **Milestones:** Store a short list of durable facts/decisions and merge into each update prompt.
- **Token budgeting:** Use a tokenizer if available; otherwise approximate via character count (e.g., 4 chars ≈ 1 token).

#### Step 2.1: Update Summarization Logic
**File:** `src/summarize.rs`

Replace the entire `summarize_channel()` method:

```rust
pub async fn summarize_channel(&self, channel_id: &str, days_lookback: i64) -> anyhow::Result<()> {
    info!("Starting rolling summarization for channel: {}", channel_id);

    // Step 1: Fetch previous summary (context for rolling window)
    let previous_summary = self.db.get_latest_summary(channel_id).await.ok().flatten();

    // Step 2: Fetch recent messages (no embeddings required)
    let messages = self.db
        .get_recent_messages(
            channel_id,
            Utc::now() - Duration::days(days_lookback),
            100, // Summarize up to 100 recent messages
        )
        .await?;

    // Optional: fetch milestone anchors (decisions, facts)
    let milestones = self.db.get_channel_milestones(channel_id).await.unwrap_or_default();

    if messages.is_empty() {
        info!("No new messages to summarize in channel: {}", channel_id);
        return Ok(());
    }

    // Step 3: Format messages chronologically
    let mut formatted_messages = String::new();
    for msg in messages.iter().rev() {  // Reverse to get chronological order
        formatted_messages.push_str(&format!(
            "[{}] <@{}>: {}\n",
            msg.timestamp, msg.user_id, msg.content
        ));
    }

    // Step 4: Build prompt for rolling window update
    let prompt = if let Some(prev_summary) = &previous_summary {
        // Update existing summary with new context
        format!(
            "You are maintaining a conversation summary. Update the following summary based on new messages. \
             Keep continuity and only add important new information. \
             Output a single cohesive paragraph.\n\n\
             PREVIOUS SUMMARY:\n{}\n\n\
             MILESTONES:\n{}\n\n\
             NEW MESSAGES (last {} hours):\n{}\n\n\
             UPDATED SUMMARY:",
            prev_summary,
            milestones.join("\n"),
            days_lookback * 24,
            formatted_messages
        )
    } else {
        // First summary
        format!(
            "Summarize the following chat history in a single paragraph. \
             Focus on key topics, decisions, and important context.\n\n\
             MILESTONES:\n{}\n\n\
             MESSAGES:\n{}\n\n\
             SUMMARY:",
            milestones.join("\n"),
            formatted_messages
        )
    };

    // Step 5: Call LLM to generate updated summary
    let new_summary = self.llm.completion(&prompt).await?;

    // Step 5b: Enforce a hard size cap (optional second compression pass)
    let new_summary = self.enforce_summary_cap(&new_summary, 1200).await?;

    // Step 6: Save to DB
    self.db.save_summary(channel_id, &new_summary).await?;

    info!(
        "Successfully summarized channel {}. Summary length: {} chars",
        channel_id,
        new_summary.len()
    );
    Ok(())
}

// Helper: Get all active channels
pub async fn get_active_channels(&self) -> anyhow::Result<Vec<String>> {
    // Query DB for distinct channel IDs with recent activity
    self.db.get_channels_with_activity().await
}
```

**Add to `SummarizationManager`:**
- `enforce_summary_cap(summary, max_tokens)` to compress oversized summaries (LLM pass or heuristic).

**Add to `Database`:**
- `get_recent_messages(channel_id, from, limit)` (time-based fetch, no embeddings required).
- `get_channel_milestones(channel_id)` (table `channel_milestones`: `channel_id`, `milestone`, `created_at`).

**Example (`get_channels_with_activity`):**
```rust
pub async fn get_channels_with_activity(&self) -> anyhow::Result<Vec<String>> {
    let conn = self.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT channel_id FROM messages 
         WHERE timestamp > datetime('now', '-7 days')
         ORDER BY MAX(timestamp) DESC"
    )?;
    let channels = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(channels)
}
```

---

#### Step 2.2: Wire Up Background Loop
**File:** `src/main.rs` (Lines 240-250)

Current (empty):
```rust
// Start background summarization task (runs every 4 hours)
let db_clone = db.clone();
let llm_clone = llm_client.clone();
tokio::spawn(async move {
    let _manager = mascord::summarize::SummarizationManager::new(db_clone, llm_clone);
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(4 * 3600));
    
    loop {
        interval.tick().await;
        // For now, we summarize the current channel history or all active channels
        // A more advanced version would query unique channel_ids from the DB
        info!("Triggering periodic background summarization...");
        // TODO: Implement multi-channel discovery for summarization
        // For MVP, we've enabled manual trigger via /settings context summarize
    }
});
```

New:
```rust
// Start background summarization task (runs every 4 hours)
let db_clone = db.clone();
let llm_clone = llm_client.clone();
tokio::spawn(async move {
    let manager = mascord::summarize::SummarizationManager::new(db_clone, llm_clone);
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(4 * 3600));
    
    loop {
        interval.tick().await;
        info!("Starting periodic background summarization cycle...");
        
        match manager.get_active_channels().await {
            Ok(channels) => {
                if channels.is_empty() {
                    debug!("No active channels found for summarization");
                    continue;
                }
                
                for channel_id in channels {
                    if let Err(e) = manager.summarize_channel(&channel_id, 1).await {
                        // Summarize messages from last 24 hours
                        error!(
                            "Failed to summarize channel {}: {}",
                            channel_id, e
                        );
                    }
                }
                
                info!("Completed summarization cycle");
            }
            Err(e) => {
                error!("Failed to fetch active channels: {}", e);
            }
        }
    }
});
```

---

### Phase 3: Music Reliability (Priority 2 - Tactical)

**Goal:** Fix YouTube cookie handling to actually work with age-restricted content, and **surface a clear error** when cookies are missing and the video is age-restricted.

**File:** `src/commands/music.rs` (Lines 60-100)

Current (broken):
```rust
let source = YoutubeDl::new(ctx.data().http_client.clone(), url.clone());
info!("Queueing audio for guild {}: {}", guild_id, url);
handler.enqueue_input(source.into()).await;
```

With cookie handling attempt:
```rust
if let Some(cookies) = &ctx.data().config.youtube_cookies {
    if std::path::Path::new(cookies).exists() {
        std::env::set_var("YTDL_ARGS", format!("--cookies {}", cookies));
    } else {
        warn!("YOUTUBE_COOKIES set but file not found at '{}'; skipping cookies", cookies);
    }
}
```

New (recommended):
```rust
use songbird::input::Compose;

let cookies_path = ctx.data().config.youtube_cookies.clone();
let cookies_ok = cookies_path
    .as_deref()
    .is_some_and(|p| std::path::Path::new(p).exists());

let is_url = url.starts_with("http://") || url.starts_with("https://");
let mut source = if is_url {
    YoutubeDl::new(ctx.data().http_client.clone(), url.clone())
} else {
    YoutubeDl::new_search(ctx.data().http_client.clone(), url.clone())
};

let mut args = vec!["--no-playlist".to_string()];
if let (Some(path), true) = (cookies_path.as_ref(), cookies_ok) {
    args.push("--cookies".to_string());
    args.push(path.clone());
}
source = source.user_args(args);

// Preflight to surface age restriction errors to the user.
source.aux_metadata().await?;

info!("Queueing audio for guild {}: {}", guild_id, url);
handler.enqueue_input(source.into()).await;
```

**Error Behavior (Required):**
- If no cookies are configured and `yt-dlp` returns an age-restricted error, **surface a user-visible error** indicating cookies are required.
- Do **not** silently fail or only log warnings.

**Verification:**
- Add a test with a known age-restricted video URL
- Verify that bot can play it (or at least attempt without 403 error)

---

### Phase 4: Agent Safety (Priority 2 - Future-Proofing)

**Goal:** Add confirmation gates for dangerous operations.

**File:** `src/tools/mod.rs`

Update `Tool` trait:
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    
    // NEW: Safety tier
    fn requires_confirmation(&self) -> bool {
        false  // Default: safe
    }
    
    async fn execute(&self, params: Value) -> Result<Value>;
}
```

Update tools like `ShutdownTool`:
```rust
impl Tool for ShutdownTool {
    // ...
    fn requires_confirmation(&self) -> bool {
        true  // Dangerous operation
    }
}
```

**File:** `src/llm/agent.rs`

Update `execute_tool_call()` (requires a `ToolExecutionContext` or command mediator):
```rust
async fn execute_tool_call(
    &self,
    tool_call: &ChatCompletionMessageToolCall,
    available_tools: &[Arc<dyn Tool>],
    ctx: Context<'_>,  // NEW: Discord context for sending buttons
) -> anyhow::Result<Value> {
    let name = &tool_call.function.name;
    let tool = available_tools.iter().find(|t| t.name() == name)
        .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;
    
    // Check if confirmation is needed
    if tool.requires_confirmation() {
        info!("Tool '{}' requires confirmation from user", name);
        
        // Send Discord message with Confirm/Cancel buttons
        let message = ctx.send(
            poise::CreateReply::default()
                .content(format!("🤖 I need your permission to execute: **{}**", name))
                .components(vec![
                    CreateActionRow::Buttons(vec![
                        CreateButton::new("confirm_tool").label("✅ Confirm").style(ButtonStyle::Green),
                        CreateButton::new("cancel_tool").label("❌ Cancel").style(ButtonStyle::Red),
                    ])
                ])
        ).await?;
        
        // Wait for user interaction (5 minute timeout)
        if let Some(interaction) = message.collector(ctx.serenity_context())
            .timeout(Duration::from_secs(300))
            .next()
            .await
        {
            if interaction.data.custom_id == "cancel_tool" {
                interaction.defer(ctx.serenity_context()).await?;
                return Err(anyhow::anyhow!("Tool execution cancelled by user"));
            }
            interaction.defer(ctx.serenity_context()).await?;
        } else {
            return Err(anyhow::anyhow!("Confirmation timeout"));
        }
    }
    
    // Execute the tool
    tool.execute(arguments).await
}
```

---

## Part 4: Testing Strategy

### Unit Tests (Per Component)

**`src/db/mod.rs`:**
- [x] Embedding serialization/deserialization (already in code)
- [ ] Cosine similarity calculation (new)
- [ ] Vector search returns correct top-K (new)

**`src/summarize.rs`:**
- [ ] Rolling window prompt generation (new)
- [ ] Multi-channel summarization (new)

### Integration Tests

**`tests/vector_search_integration.rs` (new):**
```rust
#[tokio::test]
async fn test_vector_search_similarity() {
    // Create DB
    // Insert 3 messages with embeddings
    // Query with similar embedding
    // Verify top result is highest similarity
}
```

**`tests/summarization_workflow.rs` (new):**
```rust
#[tokio::test]
async fn test_rolling_summary_preservation() {
    // Create 2 batches of messages
    // Summarize batch 1
    // Summarize batch 1+2
    // Verify summary 2 includes context from summary 1
}
```

---

## Part 5: Deployment & Rollout

### Backward Compatibility
- `embedding` column is nullable (messages added before Phase 1 will have NULL)
- Search will gracefully skip messages without embeddings
- Gradual backfill: next time embeddings are generated, they're computed and stored

### Deployment Order
1. **Phase 1** (Vector Search) - Deploy schema changes + helpers
2. **Phase 2** (Summarization) - Deploy new background loop
3. **Phase 3** (Music) - Deploy cookie fix
4. **Phase 4** (Agent Safety) - Deploy confirmation gates (lowest priority)

### Monitoring
- Add metrics: "messages indexed", "average vector search latency", "summarization success rate"
- Log all tool execution (especially ones requiring confirmation)

---

## Part 6: Documentation Updates

After implementation, update:
- `docs/ARCHITECTURE.md` - Explain vector search layer
- `docs/COMPONENT_RAG_DOCS.md` - Detail embedding strategy
- `docs/REQUIREMENTS.md` - Mark "R-003 RAG" as fully implemented
- `docs/GAP_ANALYSIS.md` - Remove GAP-001, GAP-008, GAP-009 (mark resolved)

---

## Summary: What We're Building

| Phase | Component | Work | Impact | Timeline |
|-------|-----------|------|--------|----------|
| **1** | Vector Search | Replace keyword → semantic | 🎯 Enables true RAG | 2-3 days |
| **1** | Summarization | Activate rolling window | 📚 Working Memory works | 1-2 days |
| **2** | Music | Fix cookies | 🎵 Age-restricted videos work | 2 hours |
| **2** | Agent Safety | Add confirmations | 🔒 Safety guardrails | 1 day |

**Total effort:** ~1 week for full implementation.

**Outcome:** From "mock AI system" to "functional agentic Discord bot with real memory."

---

## Addendum: Review Notes & Best Practices (2026-02-03)

This section captures corrections and best practices from an external review with online references.

### Corrections to Proposed Steps

1. **Summarization should not use vector search**  
   The proposed `summarize_channel()` calls `search_messages("", vec![], ...)`. Once vector search is live, this will return *no results* because embeddings are required.  
   **Fix:** Add a dedicated `get_recent_messages(channel_id, from_date, limit)` query that reads by `timestamp` only and does **not** require embeddings.

2. **`get_channels_with_activity()` SQL is invalid**  
   Current draft uses `ORDER BY MAX(timestamp)` without `GROUP BY`.  
   **Fix:**  
   ```sql
   SELECT channel_id
   FROM messages
   WHERE timestamp > datetime('now', '-7 days')
   GROUP BY channel_id
   ORDER BY MAX(timestamp) DESC
   ```

3. **Parameter binding in dynamic SQL**  
   The draft builds a `Vec<&dyn ToSql>` containing references to temporaries (e.g., `from.format(...).to_string()`), which will not live long enough.  
   **Fix:** Use owned values with `Vec<Box<dyn ToSql>>` or `rusqlite::params_from_iter`.

4. **Timestamp storage consistency**  
   The roadmap uses `datetime(?6, 'unixepoch')` inserts but compares against `memory_start_date`.  
   **Fix:** Decide on one representation:  
   - **Option A**: Store as `INTEGER` (Unix epoch) and compare numerically.  
   - **Option B**: Store as ISO-8601 strings and compare lexicographically.  
   Avoid mixing formats.

5. **Indexing for performance**  
   Add indexes to speed channel/date filters before any vector scoring:  
   ```sql
   CREATE INDEX IF NOT EXISTS idx_messages_channel_time
     ON messages(channel_id, timestamp);
   CREATE INDEX IF NOT EXISTS idx_messages_guild_time
     ON messages(guild_id, timestamp);
   ```

### Best-Practice Notes (From Online Sources)

1. **Chunking strategy**  
   For retrieval quality, chunk by semantic boundaries when possible and use overlap to preserve context across boundaries. A baseline is ~100–200 words per chunk with overlap (10–20% of chunk size).  

2. **Metadata for filtering**  
   Store retrieval metadata (guild, channel, message ID, timestamp) to filter and order results before scoring vectors.

3. **Cookie files for `yt-dlp`**  
   Ensure the cookies file is in Netscape/Mozilla format and has correct newline encoding (LF on Unix/macOS). Invalid formatting often causes 400 errors.

4. **Cosine similarity definition**  
   The cosine similarity formula in the roadmap is correct; keep it, but ensure vector dimensions match and handle zero norms safely.

### Optional Enhancements (If Needed Later)

1. **Hybrid retrieval**  
   Combine keyword + vector ranking (e.g., query expansion + vector rerank) if semantic-only retrieval misses short/rare terms.

2. **Embedding backfill job**  
   Add a background task to generate embeddings for older messages when the bot is idle.
