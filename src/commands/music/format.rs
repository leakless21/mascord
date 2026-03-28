//! Display helpers for queue and now-playing embeds.

use std::time::Duration;

pub(crate) fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    } else {
        s.to_string()
    }
}

pub(crate) fn format_hms(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

pub(crate) fn format_pos_dur(pos: Option<Duration>, dur: Option<Duration>) -> String {
    match (pos, dur) {
        (Some(p), Some(d)) => format!("`{}` / `{}`", format_hms(p), format_hms(d)),
        (Some(p), None) => format!("`{}` / `--:--`", format_hms(p)),
        _ => "`--:--`".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_leaves_short_unchanged() {
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn truncate_inserts_ellipsis() {
        let s = "abcdefghijklmnopqrst";
        let t = truncate(s, 5);
        assert!(t.ends_with('…'));
        assert!(t.chars().count() <= 5);
    }

    #[test]
    fn format_hms_under_one_hour() {
        assert_eq!(format_hms(Duration::from_secs(65)), "1:05");
    }

    #[test]
    fn format_hms_with_hours() {
        assert_eq!(format_hms(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn format_pos_dur_both() {
        let x = format_pos_dur(Some(Duration::from_secs(30)), Some(Duration::from_secs(90)));
        assert!(x.contains("0:30"));
        assert!(x.contains("1:30"));
    }
}
