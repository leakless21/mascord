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
