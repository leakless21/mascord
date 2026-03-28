# Installation Guide

This guide covers local installation for development and single-host runtime.

## Prerequisites

- Rust toolchain
- `ffmpeg`
- `yt-dlp`
- `cmake`
- `pkg-config`

## Configure environment

```bash
cp .env.example .env
```

Required variables:

- `DISCORD_TOKEN`
- `APPLICATION_ID`
- `LLAMA_URL`
- `LLAMA_MODEL`
- `DATABASE_URL`

Recommended:

- `OWNER_ID`
- `EMBEDDING_URL`
- `EMBEDDING_MODEL`
- `EMBEDDING_API_KEY`
- `SEARXNG_URL`

## Build

```bash
cargo build --release
```

## First-time command registration

For a **test guild** (fast iteration):

```bash
./scripts/register-commands.sh <DEV_GUILD_ID>
```

For **global** commands (production-style):

```bash
./scripts/register-commands.sh --global
```

Keep `REGISTER_COMMANDS=false` for normal runs. Full workflow, API verification, and caveats: [Deployment §3](DEPLOYMENT.md#3-register-slash-commands-safely).

## Run

```bash
./bot.sh
```

or:

```bash
cargo run --release
```

## Next

- See [Deployment](DEPLOYMENT.md) for service-managed production.
- See [Command reference](COMMANDS.md) for usage.
