# Dependency Update Summary - February 4, 2026

## Overview
Successfully updated all dependencies to latest compatible versions with a critical fix for the Brave Search MCP server integration.

## Critical Fix: rmcp 0.1 → 0.14

### Problem
- MCP tool discovery was hanging indefinitely when connecting to the Brave Search server
- Root cause: rmcp 0.1 had a protocol handling bug in `list_tools` response parsing
- Bot would report "successfully connected" but then hang while discovering tools

### Solution
- Updated rmcp from v0.1.5 to v0.14.0
- This version includes proper JSON-RPC protocol handling and message parsing
- Verified with direct MCP protocol tests that Brave Search server responds correctly

## Dependencies Updated

### Direct Dependencies
| Package | Old | New | Notes |
|---------|-----|-----|-------|
| rmcp | 0.1 | 0.14 | **CRITICAL** - Fixes tool discovery hang |
| async-openai | 0.28 | 0.29 | Minor version update |
| poise | 0.6 | 0.6.1 | Patch update |
| serenity | 0.12 | 0.12.5 | Patch update |

### Transitive Dependencies
- 17+ transitive dependencies updated to latest compatible patch versions
- All updates are patch-level with no breaking changes
- Includes important security updates for cryptography libraries

## Code Changes

### src/mcp/client.rs
Updated MCP client code for rmcp 0.14 API changes:

1. **Import Updates**
   - Changed `CallToolRequestParam` → `CallToolRequestParams`
   - Updated transport imports

2. **Connection Handling**
   - Fixed `TokioChildProcess::new()` to accept owned `Command` instead of `&mut Command`

3. **Tool Discovery**
   - Changed `list_tools(Default::default())` → `list_all_tools()`
   - Added explicit timeout handling with proper error variants
   - Tool descriptions now properly handled as `Option<Cow<str>>`

4. **Tool Execution**
   - Updated `CallToolRequestParams` struct initialization
   - Added required `meta` and `task` fields

## Testing & Verification

✅ **All 20 library tests pass**
- cache tests (LRU, channel history, cleanup)
- config tests (defaults, missing vars)
- context tests (retrieval, limits, retention)
- database tests (init, search, summaries, settings)
- hybrid search tests

✅ **No clippy warnings or errors**

✅ **Release build successful**
- Binary compiles cleanly: `target/release/mascord`
- Starts up successfully with configuration

## Cleanup

- ✓ Removed temporary test files: `test_brave_*.js`, `test_mcp.rs`
- ✓ Removed test binary: `src/bin/test_mcp_discovery.rs`
- ✓ Repository is clean with only production code

## Next Steps

The bot is now ready to:
1. Successfully connect to Brave Search MCP server
2. Discover tools without hanging
3. Execute web search and local search operations

To test the Brave Search integration:
```bash
cargo build --release
./target/release/mascord
```

Then in Discord, test with: `/chat search for [query]`
