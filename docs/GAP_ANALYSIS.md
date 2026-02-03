# Gap Analysis: Mascord Discord Bot

This document tracks identified gaps, edge cases, and potential issues requiring remediation.
Last reviewed: February 3, 2026 (hybrid retrieval, milestone extraction, retention override documented; no new runtime gaps added).

## Legend

- 🔴 **Critical**: Can cause data loss, security issues, or system failure
- 🟡 **Important**: Degraded UX or potential production issues
- 🟢 **Minor**: Nice-to-have improvements

---

## 1. YouTube Audio Service (yt-dlp)

### GAP-001: Temporary File Accumulation 🔴

**Status**: Open
**Description**: Songbird's `YoutubeDl` source uses `yt-dlp` which may cache video/audio files. No cleanup mechanism exists.
**Impact**: Disk space exhaustion over time in production.
**Resolution**: Implement cleanup task with configurable temp directory and TTL.

### GAP-002: Cookie Support Not Wired 🟡

**Status**: Open (TODO in `music.rs:62-65`)
**Description**: `YOUTUBE_COOKIES` config exists but isn't passed to `yt-dlp`.
**Impact**: Age-restricted and bot-flagged videos fail to play.
**Resolution**: Pass `--cookies` argument to yt-dlp via Songbird's customization options or wrapper script.

### GAP-003: No Download Directory Configuration 🟡

**Status**: Open
**Description**: yt-dlp downloads to default/uncontrolled location.
**Impact**: Unpredictable disk usage, cleanup difficulty.
**Resolution**: Add `YOUTUBE_DOWNLOAD_DIR` config with default `/tmp/mascord_audio/`.

### GAP-004: Playlist URL Handling Undefined 🟢

**Status**: Open
**Description**: Behavior when user provides playlist URL is untested/undocumented.
**Impact**: May queue entire playlist unexpectedly or fail.
**Resolution**: Add `--no-playlist` flag or document intended behavior.

### GAP-019: Missing Cookie File Validation 🟡

**Status**: Resolved ✅
**Description**: `YOUTUBE_COOKIES` could point to a missing file with no warning.
**Impact**: `yt-dlp` may fail (e.g., 403) without an obvious cause.
**Resolution**: Validate cookie file path and log a warning, then continue without cookies.

---

## 2. Voice Channel Management

### GAP-005: No Auto-Leave on Idle/Empty Channel 🟡

**Status**: Resolved ✅
**Description**: Bot stays in voice channel indefinitely if users leave or playback ends.
**Impact**: Resource waste, confusing UX.
**Resolution**: Implemented `IdleHandler` in `src/voice/events.rs` that auto-disconnects after configurable idle timeout.

### GAP-016: /play Does Not Auto-Join Voice Channel 🟡

**Status**: Resolved ✅
**Description**: The `/play` command previously required users to manually run `/join` first.
**Impact**: UX improved to match popular music bots by auto-joining when `/play` is invoked.
**Resolution**: Extracted join logic to `join_voice_channel_internal` and updated `/play` to call it if not connected.

---

## 3. Embedding & Multimodal Capabilities

### GAP-006: Image Attachments Not Captured 🟢

**Status**: Open (Documented Limitation)
**Description**: RAG system only indexes `message.content` text. Image attachments, embeds, and URLs not processed.
**Impact**: Cannot search for or retrieve image-based information.
**Resolution**: Document limitation. Future: Add optional multimodal embedding with CLIP-like model.

### GAP-007: LLM Vision Not Supported 🟢

**Status**: Open (Documented Limitation)
**Description**: LLM client only sends text content. Discord image URLs not forwarded.
**Impact**: Bot cannot analyze images shared in conversations.
**Resolution**: Document limitation. Future: Add `LLAMA_SUPPORTS_VISION` config and multimodal message support.

---

## 4. External Service Resilience

### GAP-008: No LLM Request Timeout 🔴

**Status**: Resolved ✅
**Description**: `async_openai` calls needed explicit timeout guards.
**Impact**: Slow/hung LLM server could block commands indefinitely.
**Resolution**: Implemented `tokio::time::timeout()` guards with configurable durations in `src/llm/client.rs`.

### GAP-009: No MCP Tool Execution Timeout 🔴

**Status**: Resolved ✅
**Description**: MCP tool calls needed timeout protection.
**Impact**: Slow MCP server could block agent loop.
**Resolution**: Implemented `tokio::time::timeout()` guards around `service.call_tool()` in `src/mcp/client.rs`.

### GAP-010: No Embedding Request Timeout 🟡

**Status**: Resolved ✅
**Description**: Embedding requests needed timeout protection.
**Impact**: Search operations could hang indefinitely.
**Resolution**: Implemented `tokio::time::timeout()` guards in `src/llm/client.rs`.

---

## 5. Error Handling & Recovery

### GAP-011: Agent Loop Max Iterations Silent Failure 🟡

**Status**: Open
**Description**: When agent exceeds `max_iterations`, error is returned but not logged/distinguished.
**Impact**: Hard to debug runaway tool loops.
**Resolution**: Add specific logging and potentially notify user of iteration limit.

### GAP-012: MCP Server Crash Recovery 🟡

**Status**: Open
**Description**: If MCP subprocess crashes, no automatic reconnection or cleanup.
**Impact**: External tools become unavailable until bot restart.
**Resolution**: Add health check and automatic reconnection with backoff.

### GAP-020: Command Errors Not Surfaced 🟡

**Status**: Resolved ✅
**Description**: Command errors were logged inconsistently and often not shown to users.
**Impact**: Users saw silent failures or no feedback when commands failed.
**Resolution**: Added a centralized Poise `on_error` handler that logs details and sends a user-facing error message.

---

## 6. Discord API & Rate Limiting

### GAP-017: Bot Hangs on Startup Rate Limit 🔴

**Status**: Resolved ✅
**Description**: Serenity's default behavior is to wait and retry on 429s. If hit with a long rate limit (e.g., 1900s), the bot appears hung during startup without a clear error.
**Impact**: Poor UX, difficult to debug "silent" startup failures.
**Resolution**: Implemented a pre-check using `reqwest` in `main.rs` that explicitly detects 429s and aborts startup with a clear message if a rate limit is active.

### GAP-018: Cloudflare IP Ban from Excessive Restarts 🔴

**Status**: Open
**Description**: Frequent bot restarts (especially during development) trigger Cloudflare's Invalid Request Limit (>10,000 invalid requests in 10 minutes = IP ban for 1+ hour). Each restart makes multiple API calls (application info, command registration, gateway connection).
**Impact**: Bot completely unable to start for extended periods (1097+ seconds). Development workflow blocked.
**Root Causes**:

- `REGISTER_COMMANDS=true` causes command registration on EVERY startup (should only happen when commands change)
- No exponential backoff on failed API calls
- No check for existing command registration state
- Rapid restart cycles during development accumulate failed requests
**Resolution**:

1. Set `REGISTER_COMMANDS=false` by default in `.env.example`
2. Add command registration state tracking (hash of command signatures)
3. Implement exponential backoff for API failures
4. Add startup delay option for development
5. Document best practices: use guild commands during dev, only register globally when deploying

---

## 7. Data & Storage

### GAP-013: SQL Injection in Search Query 🔴

**Status**: Open
**Description**: `db/mod.rs:146` uses string formatting for query parameter: `format!(" AND content LIKE '%{}%'", query.replace("'", "''"))`.
**Impact**: SQL injection vulnerability despite quote escaping.
**Resolution**: Use parameterized queries with `?` placeholders.

### GAP-014: Database Connection Pool Absent 🟢

**Status**: Open
**Description**: Single `Mutex<Connection>` may become bottleneck under load.
**Impact**: Slow response times with concurrent requests.
**Resolution**: Consider `r2d2` or `deadpool` connection pool for SQLite.

---

## 7. Configuration & Security

### GAP-015: API Keys Logged in Debug Mode 🟡

**Status**: Need Verification
**Description**: `Config` derives `Debug` which may print API keys in logs.
**Impact**: Credential exposure in debug logs.
**Resolution**: Implement custom `Debug` that redacts sensitive fields.

---

## 8. Platform Support

### GAP-021: macOS Support Not Documented or Validated 🟡

**Status**: Open
**Description**: macOS is not listed as a supported platform, and there is no CI or documented validation of macOS builds/runtime behavior.
**Impact**: macOS users may hit build or runtime issues without clear guidance; support expectations are unclear.
**Resolution**: Document macOS support and prerequisites, and add CI (or a manual test checklist) to validate macOS builds and basic runtime.

---

## Resolved Issues

- [x] **GAP-001**: Temporary File Accumulation (Phase 2)
- [x] **GAP-002**: Cookie Support Wired (Phase 2)
- [x] **GAP-003**: Download Directory Config (Phase 2)
- [x] **GAP-005**: Auto-Leave on Idle (Phase 3)
- [x] **GAP-008**: LLM Request Timeout (Phase 1)
- [x] **GAP-009**: MCP Tool Execution Timeout (Phase 1)
- [x] **GAP-010**: Embedding Request Timeout (Phase 1)
- [x] **GAP-011**: Agent Loop Failure Logging (Phase 4)
- [x] **GAP-013**: SQL Injection in Search Query (Phase 1)
- [x] **GAP-015**: API Key Redaction in Debug (Phase 4/1)
- [x] **GAP-017**: Bot Hangs on Startup Rate Limit (Phase 5)
- [x] **GAP-019**: Missing Cookie File Validation (Phase 6)
- [x] **GAP-020**: Command Errors Not Surfaced (Phase 6)

---

## Test Coverage Gaps

| Component | Existing Tests | Missing Coverage |
|-----------|----------------|------------------|
| `cache.rs` | ✅ Basic insert/get | Eviction behavior |
| `config.rs` | ✅ Defaults, missing vars | Custom Debug redaction |
| `context.rs` | ✅ Context retrieval, limits | Retention time filtering |
| `db/mod.rs` | ✅ Init, save, settings | Search, summaries |
| `mcp/` | ❌ None | Connection, tool execution |
| `llm/` | ❌ None | Timeout handling, errors |
| `voice/` | ❌ None | Join/leave, queue |
