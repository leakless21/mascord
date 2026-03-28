use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

pub async fn run_health_server(port: u16, ready: Arc<AtomicBool>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
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

            let (status_line, body) = if first_line.starts_with("GET /healthz") {
                ("HTTP/1.1 200 OK", "ok")
            } else if first_line.starts_with("GET /readyz") {
                if ready.load(Ordering::SeqCst) {
                    ("HTTP/1.1 200 OK", "ready")
                } else {
                    ("HTTP/1.1 503 Service Unavailable", "not_ready")
                }
            } else {
                ("HTTP/1.1 404 Not Found", "not_found")
            };

            let response = format!(
                "{status_line}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );

            if let Err(e) = stream.write_all(response.as_bytes()).await {
                warn!("Health server write error to {addr}: {e}");
            }
        });
    }
}
