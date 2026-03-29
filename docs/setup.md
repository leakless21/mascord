# Setup and operations

Install, configure, register slash commands, deploy, and troubleshoot.

## Install

**Dependencies:** Rust, `ffmpeg`, `yt-dlp`, `cmake`, `pkg-config`, plus an OpenAI-compatible LLM.

**macOS:** `brew install rustup ffmpeg yt-dlp node cmake opus pkg-config && rustup default stable` (optional: `brew bundle`).

**Linux (e.g. Debian/Ubuntu):** `sudo apt-get install rustup ffmpeg yt-dlp nodejs cmake libopus-dev pkg-config && rustup default stable`.

```bash
cp .env.example .env
cargo build --release
mkdir -p data
./bot.sh
```

**Required in `.env`:** `DISCORD_TOKEN`, `APPLICATION_ID`, `LLAMA_URL`, `LLAMA_MODEL`, `DATABASE_URL`.

**Discord:** Enable **Message Content Intent** (Developer Portal → Bot → Privileged Gateway Intents).

**LLM examples:** Local: `LLAMA_URL=http://localhost:8080/v1`. OpenAI: `https://api.openai.com/v1` + `LLAMA_API_KEY`. OpenRouter: `https://openrouter.ai/api/v1` + key.

Full list of variables and defaults: [`.env.example`](../.env.example).

## Slash commands (registration)

Keep **`REGISTER_COMMANDS=false`** for normal runs. Re-registering on every startup can trigger Discord rate limits.

- Test guild: `./scripts/register-commands.sh <DEV_GUILD_ID>`
- Global: `./scripts/register-commands.sh` or `./scripts/register-commands.sh --global`

For global registration, the script clears `DEV_GUILD_ID` for that process so the app registers globally. Afterward set `REGISTER_COMMANDS=false` again.

Verify (do not leak tokens):

```bash
export DISCORD_TOKEN='…' APPLICATION_ID='…'
curl -sS -H "Authorization: Bot ${DISCORD_TOKEN}" \
  "https://discord.com/api/v10/applications/${APPLICATION_ID}/commands" | jq -r '.[].name' | sort
```

Optional during registration only: `HEALTH_PORT=0 ./scripts/register-commands.sh --global`

**Note:** If you `source .env` in bash, long unquoted `SYSTEM_PROMPT` values can break the shell; the app still loads `.env` via `dotenvy` at runtime.

## Production

**Recommended env:** `REGISTER_COMMANDS=false`, optional `HEALTH_PORT=8088` (`GET /healthz`, `/readyz`), `JOB_LEASES_ENABLED=true` + `JOB_LEASE_TTL_SECS=120` if multiple instances share one DB.

**Deploy script:** `./scripts/deploy.sh` — builds release and restarts `mascord` systemd if present.

**Systemd:** Edit [`systemd/mascord.service`](../systemd/mascord.service) (user, paths, `WorkingDirectory`), then:

```bash
sudo cp systemd/mascord.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now mascord
```

**Multi-instance:** Shared `DATABASE_URL`; enable job leases; register commands once, not on every boot.

## Monitoring (systemd)

Assume the unit is installed as **`mascord`** (from [`systemd/mascord.service`](../systemd/mascord.service)). Logs go to the **journal**, not a file under `/tmp`, unless you add logging elsewhere.

### Service state

```bash
sudo systemctl status mascord          # active / failed, last lines of log, PID
systemctl is-active mascord            # prints active or inactive
systemctl is-failed mascord            # failed or active (exit 0 if not failed)
systemctl show mascord -p ActiveState -p SubState -p MainPID -p Restart
```

The sample unit uses **`Restart=on-failure`** and **`RestartSec=5`**: if the process exits non-zero, systemd restarts it. A clean **`/shutdown`** from the owner still stops the service until you start it again (`systemctl start mascord`).

### Logs (primary way to see errors)

```bash
sudo journalctl -u mascord -f --no-pager              # follow live
sudo journalctl -u mascord -n 200 --no-pager          # last 200 lines
sudo journalctl -u mascord --since today -p err       # errors today
```

Mascord logs via **`tracing`**; anything at error level will show here. Use this first when the bot “dies” or misbehaves.

### After deploy or config changes

```bash
sudo systemctl daemon-reload    # only if you edited the unit
sudo systemctl restart mascord
sudo systemctl status mascord
```

### Optional: HTTP checks (`.env` / `HEALTH_PORT`)

If you set **`HEALTH_PORT`** (e.g. `8088`) in `.env` next to the service `WorkingDirectory`, the bot exposes:

| Endpoint | Meaning |
|----------|---------|
| `GET /healthz` | Process up (liveness). |
| `GET /readyz` | `200` when the bot finished startup (**readiness**); `503` until then. |

```bash
curl -fsS http://127.0.0.1:8088/readyz
```

Use this for **Uptime Kuma** (or similar) on the **same host** (`127.0.0.1`) or behind a firewall. The listener binds to **all interfaces**; do not expose it publicly without restriction. Systemd does **not** use these URLs by default; it uses the process exit code and `Restart=`.

### External alerts

For “notify me when the service is down,” rely on **systemd** (e.g. `OnFailure=` unit, or a watchdog that runs `systemctl is-active mascord`) or an HTTP check to **`/readyz`** as above—not only `systemctl status` from memory.

## Troubleshooting

| Issue | Try |
|-------|-----|
| Won’t start | `mkdir -p data/`; check `DISCORD_TOKEN` / `APPLICATION_ID` |
| DB errors | Parent dir for `DATABASE_URL` must exist |
| LLM errors | `curl` your `LLAMA_URL/models`; check `LLM_TIMEOUT_SECS`; read `journalctl -u mascord` for `LLM API error` / timeout lines |
| Commands missing | Run registration once (above), then `REGISTER_COMMANDS=false` |
| No message content | Message Content Intent + bot permissions |
| Mention/reply joins voice but does not queue audio | Use a direct `play ...` phrase or `/play`; check `yt-dlp` and `ffmpeg` availability |

## See also

- [commands.md](commands.md) — slash commands
- [internals.md](internals.md) — architecture and module map
