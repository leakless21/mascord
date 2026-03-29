//! Ensures `.env` parses through `HEALTH_PORT` (regression for Homepage probes).

use std::fs::File;

use dotenvy::Iter;

#[test]
fn env_file_parses_health_port_line() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    let iter = Iter::new(File::open(&p).expect(".env readable"));
    let mut health = None;
    for item in iter {
        let (k, v) = item.expect("line parse");
        if k == "HEALTH_PORT" {
            health = Some(v);
        }
    }
    assert_eq!(
        health.as_deref(),
        Some("8088"),
        "HEALTH_PORT missing or wrong; dotenvy may have failed on an earlier line"
    );
}
