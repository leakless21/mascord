//! Injected **context** (time), separate from the main `SYSTEM_PROMPT` in config.
//! Keeps the primary system instruction short—common pattern in agent apps (instructions vs. facts).

use chrono::{Local, Utc};

/// One-line clock context for the model (minimal tokens; both UTC and local).
pub fn get_datetime_context() -> String {
    let utc_now = Utc::now();
    let local_now = Local::now();
    format!(
        "Time: {} UTC · {} (local)",
        utc_now.format("%Y-%m-%d %H:%M"),
        local_now.format("%Y-%m-%d %H:%M %Z")
    )
}

/// Second system message: only time, not a duplicate of tool/behavior rules.
pub fn build_datetime_system_message() -> String {
    get_datetime_context()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime_context_format() {
        let context = get_datetime_context();
        assert!(context.starts_with("Time:"));
        assert!(context.contains("UTC"));
        assert!(context.contains("local"));
    }

    #[test]
    fn test_build_datetime_system_message() {
        let msg = build_datetime_system_message();
        assert!(!msg.is_empty());
        assert!(msg.starts_with("Time:"));
    }
}
