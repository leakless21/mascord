# Component: LLM Documentation

## Area of Responsibility
Communicating with local LLM servers (llama.cpp) via OpenAI-compatible endpoints.

## Key Classes / Modules
- `src/llm/client.rs`: `LlmClient` struct handling HTTP requests to llama.cpp.
- `src/llm/mod.rs`: Module exports.

## Interfaces
- **External**: llama.cpp HTTP API (OpenAI spec).
- **Internal**: Provides `chat()` and `get_embeddings()` methods to the framework.

## Implementation Details
Uses `async-openai` crate configured with a custom `api_base`.
- **URL Format**: The `api_base` must include the version prefix (e.g., `/v1`) as it is used directly by the client to construct full endpoint paths (e.g., `url + /chat/completions`). Trailing slashes should be avoided.
- **Resilience**: 120s chat timeout, 30s embedding timeout.
- **Agent**: 10-step iteration limit with improved logging and user feedback.
