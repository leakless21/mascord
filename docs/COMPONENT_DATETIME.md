# Date/Time Handling in Mascord

## Overview

The bot is now aware of the current date and time. This context is automatically injected into every LLM prompt to ensure the assistant understands when responses are being generated.

## Implementation

### System Message Injection

Current date/time is injected as a separate system message in three key chat pathways:

1. **Slash Command** (`/chat`): [src/commands/chat.rs](src/commands/chat.rs)
2. **Message Replies**: [src/reply.rs](src/reply.rs)
3. **Bot Mentions**: [src/mention.rs](src/mention.rs)

The format includes both UTC and local time for maximum clarity:

```text
Current date/time: Wednesday, February 5, 2025, 14:30:15 UTC (2025-02-05T14:30:15Z)
Local time: Wednesday, February 5, 2025, 09:30:15 EST (2025-02-05T09:30:15-05:00)
```

### Utilities Module

Date/time utilities are centralized in [src/system_prompt.rs](src/system_prompt.rs):

- `get_datetime_context()`: Returns formatted current date/time string
- `build_datetime_system_message()`: Builds a system message for inclusion in LLM prompt

## Best Practices Applied

### 1. UTC for Consistency
- Internal representation uses UTC (`Utc::now()`)
- Ensures reproducibility and prevents timezone-related bugs
- Ideal for serverless/distributed environments

### 2. Local Time for Clarity
- Local timezone also included in the message
- Helps the LLM understand user context
- Uses `chrono::Local` to automatically detect system timezone

### 3. ISO 8601 Format
- RFC 3339 timestamps enable machine parsing
- Becomes available if the LLM needs to perform time calculations
- Standard across APIs and databases

### 4. Human-Readable Format
- Day of week included (e.g., "Wednesday") for human readability
- Full date with month name (e.g., "February 5, 2025")
- Makes context clear in conversations

### 5. Separate System Message
- Date/time is injected as a separate system message from the main prompt
- Prevents conflicts with customizable system prompts
- Allows the LLM to clearly distinguish this is factual context, not instructions

## Dependencies

- `chrono`: Industry-standard Rust date/time library
  - Already included in `Cargo.toml` with `serde` feature for serialization
  - Handles timezone-aware datetime operations
  - Provides both UTC and Local timezone types

## LLM Integration Points

The date/time context flows into:

1. **Agent Loop** (`src/llm/agent.rs`): Passed in message history before calling LLM
2. **Tool Execution**: Tools can make time-dependent decisions
3. **Context Retrieval**: RAG engine could use current time to weight recent messages higher
4. **User Memory**: Expiry timestamps compared against current time

## Example Prompt Structure

```python
[
    {"role": "system", "content": "You are a helpful Discord bot assistant..."},
    {"role": "system", "content": "Current date/time: Wednesday, February 5, 2025, 14:30:15 UTC..."},
    {"role": "system", "content": "User metadata: id=123456, name=JohnDoe"},
    # ... conversation history follows
]
```

## Testing

Date/time utilities include basic tests:

```bash
cargo test system_prompt
```

Tests verify:
- Format includes key components (UTC, Local time, RFC3339)
- Message is non-empty and properly formatted

## Future Enhancements

Possible improvements:

1. **Timezone Configuration**: Allow per-guild timezone settings via `/settings timezone <tz>`
2. **Time-Based Tools**: Implement weather, event scheduling based on current time
3. **Temporal Context**: Weight RAG results based on time similarity (e.g., prefer older context for history questions)
4. **Scheduled Reminders**: Leverage current time for reminder dispatch decisions
5. **Analytics**: Log response times for performance monitoring

## Related Files

- [src/system_prompt.rs](src/system_prompt.rs): Date/time utilities
- [src/reply.rs](src/reply.rs): Reply handler with date/time injection
- [src/commands/chat.rs](src/commands/chat.rs): Chat command with date/time injection
- [src/mention.rs](src/mention.rs): Mention handler with date/time injection
- [Cargo.toml](Cargo.toml): Dependencies including `chrono`
