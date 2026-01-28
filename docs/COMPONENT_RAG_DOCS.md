# Component: RAG & Database Documentation

## Area of Responsibility
Message persistence, semantic indexing, and retrieval-augmented generation.

## Key Classes / Modules
- `src/db/mod.rs`: `Database` struct for SQLite message storage.
- `src/rag/mod.rs`: Search filters and result structures.
- `src/commands/rag.rs`: `/search` slash command.

## Interfaces
- **Storage**: SQLite (`data/mascord.db`).
- **Logic**: Vector similarity search (via `sqlite-vec`).

## State Management
Persistent message history in SQLite. On-arrival message indexing via event handlers.
