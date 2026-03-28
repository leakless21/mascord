# Deployment Guide

This guide covers safe single-host and multi-instance deployment workflows for Mascord.

## 1) Prerequisites

- Rust toolchain
- `ffmpeg`
- `yt-dlp`
- `cmake`
- `pkg-config`

## 2) Configure `.env`

Required:

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

Operational:

- `REGISTER_COMMANDS=false` for normal runtime
- `DEV_GUILD_ID=<your test guild>`
- `HEALTH_PORT=8088` to expose `/healthz` and `/readyz`
- `JOB_LEASES_ENABLED=true` for shared-db multi-instance deployments
- `JOB_LEASE_TTL_SECS=120` for background lease windows

## 3) Register slash commands safely

Mascord registers Discord application commands during bot startup when `REGISTER_COMMANDS=true`. For normal
runtime, keep **`REGISTER_COMMANDS=false`** so every restart does not rewrite command metadata.

**Guild vs global**

| Mode | When to use | Speed |
|------|-------------|--------|
| **Guild** | Day-to-day development; commands appear only in your test server | Immediate |
| **Global** | Production or any server without per-guild registration; same command tree everywhere | Discord clients can lag API by up to ~1 hour |

**Prerequisite:** `.env` must contain `DISCORD_TOKEN`, `APPLICATION_ID`, and the rest of the variables your
build expects (same as normal startup). The registration script only ensures `REGISTER_COMMANDS=true`.

### 3a) Register to a test guild (recommended first)

From the `mascord` directory:

```bash
./scripts/register-commands.sh <DEV_GUILD_ID>
```

`<DEV_GUILD_ID>` is the numeric ID of the Discord server (guild) where commands should appear.

You can also pass `-h` / `--help` for a short usage summary.

### 3b) Register globally (production alignment)

When you need the **global** command list to match the current binary (for example after removing or renaming
commands), run **without** a guild id. The script sets `DEV_GUILD_ID` to empty for that process only so Mascord
uses the global registration path; your `.env` file is not modified.

```bash
./scripts/register-commands.sh
# equivalent:
./scripts/register-commands.sh --global
```

**Why `DEV_GUILD_ID` must be empty for this run:** If `DEV_GUILD_ID` is set (including from `.env`), Mascord
registers commands **in that guild only**. The loader uses `dotenvy`, which does **not** override variables
already present in the environment, so the script exports an empty `DEV_GUILD_ID` before starting the process.

**Optional for a one-shot run:** If you do not want the HTTP health server to listen during registration, prefix:

```bash
HEALTH_PORT=0 ./scripts/register-commands.sh --global
```

Stop the bot with **Ctrl+C** once logs show registration finished and startup continued (or use `timeout` if you
prefer a bounded run).

### 3c) After registration

1. Set **`REGISTER_COMMANDS=false`** in `.env` (or your systemd `Environment=`) for steady-state operation.
2. Restart the bot the way you usually run it (`./scripts/deploy.sh`, systemd, or `cargo run --release`).

### 3d) Verify commands with the Discord API

Replace placeholders; do not commit or share your bot token.

```bash
# From repo root or mascord/, after exporting vars (example: load only what you need in your shell)
export DISCORD_TOKEN='your_bot_token'
export APPLICATION_ID='your_application_id'

curl -sS -H "Authorization: Bot ${DISCORD_TOKEN}" \
  "https://discord.com/api/v10/applications/${APPLICATION_ID}/commands" \
  | jq -r '.[].name' | sort
```

- **Global** commands: use the URL above (`/applications/{id}/commands`).
- **Guild-scoped** commands (for comparison):  
  `GET /applications/{application.id}/guilds/{guild.id}/commands`

Global registration **replaces** the application’s global command list with the set defined in the running
binary. Old command names that are no longer in code should disappear from the global list after a successful run.

### 3e) `CARGO_TARGET_DIR` (optional)

The script defaults `CARGO_TARGET_DIR` to `mascord/target` when unset so the release binary path is stable
under the project. Override if your environment requires a different target directory.

### 3f) `.env` and shell `source`

If you `source .env` in bash, **unquoted** values containing characters like `(` and `)` (for example a long
`SYSTEM_PROMPT`) can cause syntax errors. Mascord still loads `.env` via `dotenvy` at runtime. For shell
sourcing, quote those values or set them in the environment another way.

## 4) Build and deploy

```bash
./scripts/deploy.sh
```

This builds release and restarts `mascord` systemd service if it exists.

## 5) Systemd autostart (recommended)

A checked-in unit runs the **release binary** (not `cargo run`), so boot is fast and does not depend on the
Rust toolchain on `PATH`.

**1. Build once** (from `mascord/`):

```bash
CARGO_TARGET_DIR=target cargo build --release
```

**2. Edit the unit** if needed: open [`systemd/mascord.service`](../systemd/mascord.service) and set
`User=`, `Group=`, `WorkingDirectory=`, and `ExecStart=` to match your machine and checkout path.

**3. Install and enable**:

```bash
cd /home/lkless/server/mascord
sudo cp systemd/mascord.service /etc/systemd/system/mascord.service
sudo systemctl daemon-reload
sudo systemctl enable --now mascord
```

**4. Check status and logs**:

```bash
sudo systemctl status mascord
sudo journalctl -u mascord -f --no-pager
```

**Notes**

- `.env` is read automatically from `WorkingDirectory` (same as `./bot.sh`).
- `REGISTER_COMMANDS=false` is set in the unit so a mistaken `.env` value does not re-register on every boot.
- Update after code changes: `./scripts/deploy.sh` (rebuilds and restarts the unit if the service exists), or
  `CARGO_TARGET_DIR=target cargo build --release && sudo systemctl restart mascord`.

### User session alternative (no sudo for enable)

To run the bot only after you log in, use a **user** service instead:

```bash
mkdir -p ~/.config/systemd/user
cp systemd/mascord.service ~/.config/systemd/user/mascord.service
# Edit the file: remove User=/Group= lines (user units run as you).
systemctl --user daemon-reload
systemctl --user enable --now mascord
loginctl enable-linger "$(whoami)"   # optional: start at boot without an interactive login
```

## 6) Health checks

If `HEALTH_PORT` is non-zero:

- `GET /healthz` returns process liveness
- `GET /readyz` returns readiness after bot setup completes

Example:

```bash
curl -f http://127.0.0.1:8088/healthz
curl -f http://127.0.0.1:8088/readyz
```

## 7) Multi-instance guidance

For multiple instances:

- Use a shared database (`DATABASE_URL`) rather than per-node SQLite files.
- Enable `JOB_LEASES_ENABLED=true` so only one instance runs lease-protected background jobs at a time (summarization, reminder dispatch, embedding indexer).
- Keep command registration one-shot, not on every boot.

