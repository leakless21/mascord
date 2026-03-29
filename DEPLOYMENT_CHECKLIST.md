# Mascord deployment checklist

Use this after changing prompts, tools, or agent behavior.

## Pre-deploy

1. `cd mascord && cargo test`
2. `cargo build --release`
3. Confirm `SYSTEM_PROMPT` in `.env` (if set) still matches your intent; unset uses `agent_contract::DEFAULT_SYSTEM_PROMPT` from source.
4. Restart the service (e.g. `systemctl restart mascord`) and watch `journalctl -u mascord -f` for errors.

## Post-deploy smoke

1. Mention the bot with a simple factual question (no tools).
2. Trigger a tool path you care about (e.g. web search, reminder).
3. Confirm health endpoints if exposed (`/healthz`, `/readyz`).

## Rollback

- Restore previous binary or git revision, or set `SYSTEM_PROMPT` to a known-good string in `.env`.
