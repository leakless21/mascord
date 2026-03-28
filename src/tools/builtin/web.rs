use crate::tools::Tool;
use async_trait::async_trait;
use dom_smoothie::{Config as ReadabilityConfig, Readability, TextMode};
use reqwest::Url;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderMode {
    Off,
    Auto,
    Always,
}

pub struct WebSearchTool {
    pub http_client: reqwest::Client,
    pub searxng_url: String,
    pub timeout_secs: u64,
    pub default_limit: usize,
}

pub struct FetchUrlTool {
    pub http_client: reqwest::Client,
    pub timeout_secs: u64,
    pub max_chars: usize,
    pub jina_reader_base: String,
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using SearXNG and return concise, ranked results."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (1-10)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: Value) -> anyhow::Result<Value> {
        let query = params["query"]
            .as_str()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

        let requested_limit = params["limit"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(self.default_limit);
        let limit = requested_limit.clamp(1, 10);

        let endpoint = format!("{}/search", self.searxng_url.trim_end_matches('/'));
        let request = self
            .http_client
            .get(endpoint)
            .query(&[("q", query), ("format", "json")]);

        let response = timeout(Duration::from_secs(self.timeout_secs), request.send())
            .await
            .map_err(|_| anyhow::anyhow!("web_search timed out after {}s", self.timeout_secs))??;

        let response = response.error_for_status()?;
        let payload: Value = response.json().await?;
        let raw_results = payload["results"].as_array().cloned().unwrap_or_default();

        let results = raw_results
            .into_iter()
            .take(limit)
            .map(|item| {
                json!({
                    "title": item["title"].as_str().unwrap_or_default(),
                    "url": item["url"].as_str().unwrap_or_default(),
                    "snippet": item["content"].as_str().unwrap_or_default(),
                    "engine": item["engine"].as_str().unwrap_or_default(),
                    "score": item["score"].as_f64().unwrap_or(0.0)
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "query": query,
            "result_count": results.len(),
            "results": results
        }))
    }
}

#[async_trait]
impl Tool for FetchUrlTool {
    fn name(&self) -> &str {
        "fetch_url"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return extracted readable text content."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "HTTP/HTTPS URL to fetch"
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Maximum number of characters in the returned content"
                },
                "render_javascript": {
                    "type": "boolean",
                    "description": "Legacy flag. If true, always fetch with Jina AI Reader"
                },
                "auto_render": {
                    "type": "boolean",
                    "description": "If true, try local fetch first, then auto-fallback to Jina Reader when extraction quality is poor"
                },
                "render_mode": {
                    "type": "string",
                    "description": "Rendering strategy: off, auto, always"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: Value) -> anyhow::Result<Value> {
        let url = params["url"]
            .as_str()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;

        let parsed = Url::parse(url)?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(anyhow::anyhow!("Only http/https URLs are allowed"));
        }

        let max_chars = params["max_chars"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(self.max_chars)
            .clamp(512, 40_000);

        let render_mode = parse_render_mode(&params)?;
        if render_mode == RenderMode::Always {
            return self.fetch_with_jina_reader(url, max_chars).await;
        }

        let response = timeout(
            Duration::from_secs(self.timeout_secs),
            self.http_client.get(parsed).send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("fetch_url timed out after {}s", self.timeout_secs))??;

        let status = response.status();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let response = response.error_for_status()?;
        let body = response.text().await?;
        let (title, extracted, mut extraction_mode) = if content_type.contains("text/html") {
            extract_html_text(&body, &final_url)
        } else {
            (
                String::new(),
                normalize_whitespace(&body),
                "plain_text".to_string(),
            )
        };

        let mut content = extracted;
        if render_mode == RenderMode::Auto
            && should_auto_render_with_jina(&content_type, &content, Some(&body))
        {
            if let Ok(jina_result) = self.fetch_with_jina_reader(url, max_chars).await {
                return Ok(jina_result);
            }
            extraction_mode = format!("{}_auto_fallback_failed", extraction_mode);
        }

        let total_chars = content.chars().count();
        let truncated = total_chars > max_chars;
        content = content.chars().take(max_chars).collect::<String>();

        Ok(json!({
            "url": url,
            "final_url": final_url,
            "status": status.as_u16(),
            "content_type": content_type,
            "title": title,
            "content": content,
            "extraction_mode": extraction_mode,
            "truncated": truncated,
            "total_chars": total_chars
        }))
    }
}

impl FetchUrlTool {
    async fn fetch_with_jina_reader(&self, url: &str, max_chars: usize) -> anyhow::Result<Value> {
        let endpoint = format!("{}/{}", self.jina_reader_base.trim_end_matches('/'), url);
        let response = timeout(
            Duration::from_secs(self.timeout_secs.saturating_add(5)),
            self.http_client.get(endpoint).send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Jina Reader fetch timed out"))??;

        let response = response.error_for_status()?;
        let body = response.text().await?;
        let cleaned = normalize_rich_text(&body);
        let title = extract_title_from_markdown(&cleaned);
        let extraction_mode = "jina_reader_markdown";

        let total_chars = cleaned.chars().count();
        let truncated = total_chars > max_chars;
        let content = cleaned.chars().take(max_chars).collect::<String>();

        Ok(json!({
            "url": url,
            "final_url": url,
            "status": 200,
            "content_type": "text/markdown",
            "title": title,
            "content": content,
            "extraction_mode": extraction_mode,
            "truncated": truncated,
            "total_chars": total_chars
        }))
    }
}

fn parse_render_mode(params: &Value) -> anyhow::Result<RenderMode> {
    if let Some(mode) = params["render_mode"].as_str() {
        let normalized = mode.trim().to_ascii_lowercase();
        return match normalized.as_str() {
            "off" => Ok(RenderMode::Off),
            "auto" => Ok(RenderMode::Auto),
            "always" => Ok(RenderMode::Always),
            _ => Err(anyhow::anyhow!(
                "Invalid render_mode '{}'. Use one of: off, auto, always",
                mode
            )),
        };
    }

    if params["render_javascript"].as_bool().unwrap_or(false) {
        return Ok(RenderMode::Always);
    }

    if params["auto_render"].as_bool().unwrap_or(false) {
        return Ok(RenderMode::Auto);
    }

    Ok(RenderMode::Off)
}

fn should_auto_render_with_jina(
    content_type: &str,
    extracted_text: &str,
    raw_body: Option<&str>,
) -> bool {
    if !content_type.contains("text/html") {
        return false;
    }

    let len = extracted_text.chars().count();
    if len < 450 {
        return true;
    }

    let lower_extracted = extracted_text.to_ascii_lowercase();
    let challenge_markers = [
        "enable javascript",
        "turn javascript on",
        "javascript is required",
        "please wait while we verify",
        "cf-ray",
        "captcha",
        "access denied",
        "bot detection",
    ];
    if challenge_markers
        .iter()
        .any(|marker| lower_extracted.contains(marker))
    {
        return true;
    }

    if let Some(raw) = raw_body {
        let raw_lower = raw.to_ascii_lowercase();
        if raw_lower.contains("window.__next_data__")
            && (raw_lower.contains("<noscript") || raw_lower.contains("hydration"))
            && len < 1200
        {
            return true;
        }
    }

    false
}

fn extract_html_text(html: &str, source_url: &str) -> (String, String, String) {
    if let Some((title, content)) = extract_with_readability(html, source_url) {
        return (title, content, "readability_markdown".to_string());
    }

    let document = Html::parse_document(html);

    let title_selector = Selector::parse("title").expect("title selector should be valid");
    let title = document
        .select(&title_selector)
        .next()
        .map(|node| normalize_whitespace(&node.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_default();

    let body_selector = Selector::parse("body").expect("body selector should be valid");
    let body_text = document
        .select(&body_selector)
        .next()
        .map(|node| normalize_whitespace(&node.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_else(|| normalize_whitespace(html));

    (title, body_text, "dom_text_fallback".to_string())
}

fn extract_with_readability(html: &str, source_url: &str) -> Option<(String, String)> {
    let cfg = ReadabilityConfig {
        text_mode: TextMode::Markdown,
        ..Default::default()
    };

    let mut readability = Readability::new(html, Some(source_url), Some(cfg)).ok()?;
    let article = readability.parse().ok()?;
    let title = normalize_whitespace(&article.title.to_string());
    let text_content = normalize_rich_text(article.text_content.as_ref());

    if text_content.trim().is_empty() {
        return None;
    }

    Some((title, text_content))
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_rich_text(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut previous_blank = false;

    for raw_line in normalized.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if !previous_blank {
                lines.push(String::new());
                previous_blank = true;
            }
            continue;
        }
        lines.push(line.to_string());
        previous_blank = false;
    }

    lines.join("\n").trim().to_string()
}

fn extract_title_from_markdown(markdown: &str) -> String {
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_render_mode_defaults_and_legacy_flags() {
        assert_eq!(parse_render_mode(&json!({})).unwrap(), RenderMode::Off);
        assert_eq!(
            parse_render_mode(&json!({"render_javascript": true})).unwrap(),
            RenderMode::Always
        );
        assert_eq!(
            parse_render_mode(&json!({"auto_render": true})).unwrap(),
            RenderMode::Auto
        );
    }

    #[test]
    fn test_parse_render_mode_explicit_override() {
        assert_eq!(
            parse_render_mode(&json!({"render_mode": "off", "render_javascript": true})).unwrap(),
            RenderMode::Off
        );
        assert_eq!(
            parse_render_mode(&json!({"render_mode": "auto"})).unwrap(),
            RenderMode::Auto
        );
        assert_eq!(
            parse_render_mode(&json!({"render_mode": "always"})).unwrap(),
            RenderMode::Always
        );
    }

    #[test]
    fn test_parse_render_mode_invalid_value() {
        let err = parse_render_mode(&json!({"render_mode": "sometimes"}))
            .expect_err("invalid render mode should fail");
        assert!(err.to_string().contains("Invalid render_mode"));
    }

    #[test]
    fn test_auto_render_heuristics() {
        assert!(should_auto_render_with_jina(
            "text/html",
            "Too short",
            Some("<html><body>ok</body></html>")
        ));
        assert!(should_auto_render_with_jina(
            "text/html",
            "Please enable JavaScript to continue",
            None
        ));
        assert!(should_auto_render_with_jina(
            "text/html",
            "short content",
            Some("<script>window.__NEXT_DATA__={};</script><noscript>...</noscript>")
        ));
        assert!(!should_auto_render_with_jina(
            "text/plain",
            "A lot of plain text content that is fine",
            None
        ));
        assert!(!should_auto_render_with_jina(
            "text/html",
            &"a".repeat(5000),
            Some("<html><body>normal</body></html>")
        ));
    }
}
