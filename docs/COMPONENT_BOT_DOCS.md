# Component: Core Bot Documentation

## Area of Responsibility
General bot setup, command registration, and event lifecycle management.

## Key Classes / Modules
- `src/main.rs`: Entry point and Poise framework initialization.
- `src/config.rs`: Configuration handling (env vars, constants).
- `src/commands/mod.rs`: Command registration and grouping.

## Interfaces
- **External**: Discord Gateway WebSocket.
- **Internal**: Provides `Data` struct to all commands via Poise context.

## State Management
Uses Poise's shared `Data` struct (thread-safe, wrapped in `Arc` by the framework).
