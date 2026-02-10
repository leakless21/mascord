# Component: Reminder Service

## Area of Responsibility

Durable scheduling and delivery of user reminders.

## Key Classes / Modules

- `src/commands/remind.rs`: Slash commands (`/remind set`, `/remind list`, `/remind cancel`).
- `src/reminder.rs`: Reminder service validation, background dispatcher loop, and delivery behavior.
- `src/db/mod.rs`: Reminder persistence and state transitions.

## Data Model

The `reminders` table stores:

- `guild_id`, `channel_id`, `user_id`: Delivery scope and target user.
- `message`: Reminder body.
- `remind_at`: UTC delivery timestamp (`YYYY-MM-DD HH:MM:SS`).
- `status`: `pending | processing | sent | cancelled | failed`.
- `delivery_attempts`, `last_error`, `sent_at`, `cancelled_at`: Delivery audit and retry bookkeeping.

## Delivery Flow

1. User creates a reminder via `/remind set`.
2. Service validates delay/message constraints and persists the reminder.
3. Background dispatcher polls due reminders every 15 seconds.
4. Dispatcher atomically claims due reminders by switching `pending -> processing`.
5. Delivery attempts send a channel message that pings only the target user.
6. Delivery result is persisted as:
   - Success: `processing -> sent`
   - Temporary failure: `processing -> pending` (next attempt in 1 minute)
   - Max attempts reached: `processing -> failed`

## Guardrails

- Max pending reminders per user per guild: `50`.
- Max delay: `30` days (`43200` minutes).
- Minimum lead time: `10` seconds.
- Max reminder message length: `500` characters.
- Allowed mentions are restricted to the target user to prevent accidental `@everyone`/role pings.

## Scheduling Input

`/remind set` accepts natural-language `when` input:

- Relative: `in 2 days, 30 minutes`, `3 hours`, `45m`.
- Clock time: `at 5:30PM`, `at 22:15` (interpreted in UTC).
- Absolute datetime (UTC): `YYYY-MM-DD HH:MM` or `YYYY-MM-DDTHH:MM`.
- Numeric-only input is intentionally rejected; users must include units (`10 minutes`, `3 hours`).

## Error Handling

- Validation failures return explicit user-facing errors.
- Dispatcher failures are logged with reminder IDs and retry context.
- On startup, stuck `processing` reminders are reset to `pending` for recovery.
