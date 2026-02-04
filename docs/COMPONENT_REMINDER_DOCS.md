# Component: Reminders

## Area of Responsibility

Persisting and dispatching user-created reminders on a schedule.

## Key Classes / Modules

- `src/commands/reminder.rs`: Slash commands to set/list/cancel reminders.
- `src/services/reminder.rs`: Business logic for reminder persistence.
- `src/reminders.rs`: Background dispatcher that sends due reminders to Discord.
- `src/db/mod.rs`: SQLite persistence for reminders.

## Data Model

SQLite table: `reminders`

- `id` (INTEGER, PK)
- `guild_id` (TEXT)
- `channel_id` (TEXT)
- `user_id` (TEXT)
- `message` (TEXT)
- `remind_at` (DATETIME, UTC)
- `created_at` (DATETIME)
- `delivered_at` (DATETIME, nullable)

## Configuration

Environment variables (see `.env.example`):

- `REMINDER_POLL_INTERVAL_SECS` (default `30`): Poll interval for due reminders.
- `REMINDER_BATCH_SIZE` (default `25`): Max reminders sent per poll cycle.

## Flow

1. User runs `/reminder set` with a duration and message.
2. `ReminderService` validates inputs and writes a new reminder row to SQLite.
3. `ReminderDispatcher` polls for due reminders on an interval.
4. Dispatcher sends a message in the originating channel and marks the reminder delivered.

## Error Handling

- Validation failures return user-friendly messages in the command response.
- Dispatch failures are logged and do not crash the scheduler loop.

## Security & Abuse Controls

- Reminders are owned by the creating user and can only be canceled by that user.
- Reminder delivery only mentions the requester (no role/everyone pings).
