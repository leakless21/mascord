use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

/// Normalized path (no query), e.g. `/homepage`. Handles origin-form (`GET /path`) and
/// absolute-form (`GET http://host:8088/path`) used by some HTTP clients (e.g. Homepage's proxy).
fn path_from_request_line(line: &str) -> Option<&str> {
    let target = line.split_whitespace().nth(1)?;
    let path_part = if target.starts_with("http://") || target.starts_with("https://") {
        let rest = target
            .strip_prefix("http://")
            .or_else(|| target.strip_prefix("https://"))?;
        // authority/path — first `/` after the host[:port] begins the path
        rest.find('/').map(|i| &rest[i..]).unwrap_or("/")
    } else {
        target
    };
    let path = path_part.split('?').next()?;
    if path.len() > 1 {
        Some(path.trim_end_matches('/'))
    } else {
        Some(path)
    }
}

pub async fn run_health_server(port: u16, ready: Arc<AtomicBool>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    let started = Instant::now();
    info!("Health server listening on 0.0.0.0:{port}");

    loop {
        let (mut stream, addr) = listener.accept().await?;
        let ready = ready.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let n = match stream.read(&mut buf).await {
                Ok(n) => n,
                Err(e) => {
                    debug!("Health server read error from {addr}: {e}");
                    return;
                }
            };

            if n == 0 {
                return;
            }

            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or_default();
            let method = first_line.split_whitespace().next().unwrap_or("");
            let path = path_from_request_line(first_line);

            let (status_line, body, content_type): (&str, String, &str) = if method != "GET" {
                (
                    "HTTP/1.1 405 Method Not Allowed",
                    "method_not_allowed".to_string(),
                    "text/plain; charset=utf-8",
                )
            } else if path == Some("/healthz") {
                (
                    "HTTP/1.1 200 OK",
                    "ok".to_string(),
                    "text/plain; charset=utf-8",
                )
            } else if path == Some("/readyz") {
                if ready.load(Ordering::SeqCst) {
                    (
                        "HTTP/1.1 200 OK",
                        "ready".to_string(),
                        "text/plain; charset=utf-8",
                    )
                } else {
                    (
                        "HTTP/1.1 503 Service Unavailable",
                        "not_ready".to_string(),
                        "text/plain; charset=utf-8",
                    )
                }
            } else if path == Some("/homepage") {
                let is_ready = ready.load(Ordering::SeqCst);
                let payload = serde_json::json!({
                    "state": if is_ready { "ready" } else { "starting" },
                    "uptime_seconds": started.elapsed().as_secs(),
                });
                (
                    "HTTP/1.1 200 OK",
                    payload.to_string(),
                    "application/json; charset=utf-8",
                )
            } else {
                (
                    "HTTP/1.1 404 Not Found",
                    "not_found".to_string(),
                    "text/plain; charset=utf-8",
                )
            };

            let response = format!(
                "{status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );

            if let Err(e) = stream.write_all(response.as_bytes()).await {
                warn!("Health server write error to {addr}: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::path_from_request_line;

    #[test]
    fn path_origin_form() {
        assert_eq!(
            path_from_request_line("GET /homepage HTTP/1.1"),
            Some("/homepage")
        );
        assert_eq!(
            path_from_request_line("GET /readyz?x=1 HTTP/1.1"),
            Some("/readyz")
        );
    }

    #[test]
    fn path_absolute_form_like_homepage_proxy() {
        assert_eq!(
            path_from_request_line("GET http://host.docker.internal:8088/homepage HTTP/1.1"),
            Some("/homepage")
        );
    }
}
