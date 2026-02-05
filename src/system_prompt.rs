//! System message utilities for constructing context-aware prompts
//!
//! Provides helper functions to build system messages with current date/time context,
//! ensuring the LLM is aware of when responses are being generated.

use chrono::{Local, Utc};

/// Format current date and time for inclusion in system prompts
///
/// # Best Practices Applied:
/// - Uses UTC internally for consistency and reproducibility
/// - Presents both UTC and local time for clarity
/// - Includes day of week for human readability
/// - ISO 8601 format for machine readability
///
/// # Examples:
/// ```text
/// Current date/time: Wednesday, February 5, 2025, 14:30:15 UTC (2025-02-05T14:30:15Z)
/// Local time: Wednesday, February 5, 2025, 09:30:15 EST (2025-02-05T09:30:15-05:00)
/// ```
pub fn get_datetime_context() -> String {
    let utc_now = Utc::now();
    let local_now = Local::now();

    format!(
        "Current date/time: {}, {} UTC ({})\nLocal time: {}, {} ({})",
        utc_now.format("%A, %B %d, %Y"),
        utc_now.format("%H:%M:%S"),
        utc_now.to_rfc3339(),
        local_now.format("%A, %B %d, %Y"),
        local_now.format("%H:%M:%S %Z"),
        local_now.to_rfc3339()
    )
}

/// Build a system message containing current date/time context
///
/// This should be injected into the message history to make the LLM aware
/// of the current time when making decisions.
///
/// # Returns:
/// A formatted string suitable for use as a ChatCompletionRequestSystemMessage content
pub fn build_datetime_system_message() -> String {
    format!("{}", get_datetime_context())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime_context_format() {
        let context = get_datetime_context();
        // Just verify it contains key components
        assert!(context.contains("Current date/time:"));
        assert!(context.contains("UTC"));
        assert!(context.contains("Local time:"));
        assert!(context.contains("T"));  // RFC3339 format includes 'T'
    }

    #[test]
    fn test_build_datetime_system_message() {
        let msg = build_datetime_system_message();
        assert!(!msg.is_empty());
        assert!(msg.contains("Current date/time:"));
    }
}
