# Component: Tools & agent

## Domain

The tooling layer registers **built-in** callable functions for the LLM agent (`/chat`, reply-to-bot, and mentions use the same agent loop).

### Key types

| Type | Location | Role |
|------|----------|------|
| `Tool` | `src/tools/mod.rs` | Trait for callable tools (name, schema, `execute`, optional `requires_confirmation`). |
| `ToolRegistry` | `src/tools/mod.rs` | Holds all registered tools; the agent lists them for the model. |
| `Agent` | `src/llm/agent.rs` | Multi-turn loop: model tool calls → execute → feed results back until a final reply. |

## Flow

1. The agent collects tool definitions from `ToolRegistry`.
2. Definitions are sent to `LlmClient` in OpenAI-style tool format.
3. The model may return tool calls; each is executed and results are appended to the conversation.
4. Tools may request **interactive confirmation** in Discord (`src/llm/confirm.rs`) when `requires_confirmation()` is true (for example the admin shutdown tool).

## Built-in tools (representative)

Implementations live under `src/tools/builtin/`. Examples include local history search, user memory fetch, web search/fetch, music control, and admin actions—see code for the authoritative list and schemas.

## Platform notes

- Tools that shell out or call the network depend on host connectivity and configured URLs (`SEARXNG_URL`, etc.).
- Voice-related tools require `yt-dlp` / `ffmpeg` on `PATH` like the rest of the voice stack.
