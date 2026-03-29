//! Detects obvious music play phrases for the fast path (mention/reply), avoiding LLM tool calls.

/// Returns the search/query string if `message` starts with a play-style prefix.
pub fn extract_direct_play_query(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();
    let prefixes = [
        "play me ",
        "play the ",
        "play ",
        "queue ",
        "put on ",
        "add to queue ",
    ];
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            let q = trimmed[prefix.len()..]
                .trim_matches(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .trim();
            if !q.is_empty() {
                return Some(q.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::extract_direct_play_query;

    #[test]
    fn extracts_basic_play_query() {
        assert_eq!(
            extract_direct_play_query("play under the bridge"),
            Some("under the bridge".to_string())
        );
        assert_eq!(
            extract_direct_play_query("play me under the bridge"),
            Some("under the bridge".to_string())
        );
        assert_eq!(
            extract_direct_play_query("play the national anthem"),
            Some("national anthem".to_string())
        );
    }

    #[test]
    fn extracts_queue_prefixes() {
        assert_eq!(
            extract_direct_play_query("queue \"radiohead - creep\""),
            Some("radiohead - creep".to_string())
        );
        assert_eq!(
            extract_direct_play_query("put on never gonna give you up"),
            Some("never gonna give you up".to_string())
        );
        assert_eq!(
            extract_direct_play_query("add to queue daft punk one more time"),
            Some("daft punk one more time".to_string())
        );
    }

    #[test]
    fn ignores_non_music_text() {
        assert_eq!(extract_direct_play_query("can you help me"), None);
        assert_eq!(extract_direct_play_query("play"), None);
    }
}
