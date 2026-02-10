# Component: RAG & Database Documentation

## Area of Responsibility
Message persistence, semantic indexing, and retrieval-augmented generation.

## Key Classes / Modules
- `src/db/mod.rs`: `Database` struct for SQLite message storage.
- `src/rag/mod.rs`: Search filters and result structures.
- `src/commands/rag.rs`: `/search` slash command.
- `src/tools/builtin/rag.rs`: Agent tool with summary + source provenance.

## Interfaces
- **Storage**: SQLite (`data/mascord.db`).
- **Logic**: Vector similarity search in Rust over SQLite-stored embeddings (optional `sqlite-vec` acceleration).
- **Retrieval**: Hybrid merge of vector + keyword results with dedupe; vector scoring applies a small recency boost.
- **Provenance**: Search outputs include timestamps and channel IDs; agent tool responses include a `sources` list.

## State Management
Persistent message history in SQLite. On-arrival message indexing via event handlers.

## Platform Notes
- SQLite uses the bundled library; no system SQLite dependency is required on macOS or Linux.
