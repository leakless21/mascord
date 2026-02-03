# Component: Tools & MCP System

## Domain: Tooling
The tooling domain handles the registration, discovery, and execution of callable functions (Tools) by the LLM. It supports both built-in static tools and dynamic external tools via MCP.

### Key Classes

| Class | Location | Responsibility |
|-------|----------|----------------|
| `Tool` | `src/tools/mod.rs` | Trait defining the interface for all callable tools. |
| `ToolRegistry` | `src/tools/mod.rs` | Collection and management of available tools. |
| `Agent` | `src/llm/agent.rs` | Core execution loop handling multi-turn tool calling. |
| `McpClientManager` | `src/mcp/client.rs` | Manages connections to external MCP servers. |
| `McpToolWrapper` | `src/mcp/client.rs` | Dynamic tool wrapper for MCP-provided functions. |

## Tool Calling Flow

1. The `Agent` (invoked via `/chat`) gathers all tools from `ToolRegistry` and `McpClientManager`.
2. Tool definitions (OpenAI format) are sent to the `LlmClient`.
3. The LLM returns a sequence of tool calls.
4. The `Agent` executes the tools and feeds results back to the LLM.
5. This repeats until a final answer is generated or limits are reached.

## Built-in Tools

- `play_music`: Triggers YouTube playback via Songbird/yt-dlp.
- `search_local_history`: Performs RAG search over indexed Discord messages.
- `shutdown`: Admin tool for graceful bot termination.

## MCP Integration
Configured via `mcp_servers.toml` (auto-created) or the `MCP_SERVERS` environment variable. 

Supported servers include:
- `brave-search`: Web search capabilities.
- `fetch`: Web content retrieval and markdown conversion.

### Runtime Management

The bot owner can manage MCP servers directly from Discord:
- `/mcp list`: Show all configured and active servers.
- `/mcp add`: Add a new stdio-based server (persists to TOML).
- `/mcp remove`: Remove a server and disconnect its tools.

Supports `stdio` transport for running local scripts/binaries as tool providers.

## Platform Notes
- MCP tools may require external runtimes (for example, Node.js when using `npx`-based servers).
- Ensure tool binaries are available on the host OS `PATH` for macOS and Linux deployments.
