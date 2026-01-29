use std::fs;
use std::time::{Duration, SystemTime};
use tokio::time::interval;
use tracing::{info, warn, debug};

pub async fn start_cleanup_task(download_dir: String, max_age_secs: u64) {
    info!("Starting YouTube temporary file cleanup task for directory: {}", download_dir);
    let mut ticker = interval(Duration::from_secs(300)); // Check every 5 min
    
    loop {
        ticker.tick().await;
        if let Err(e) = cleanup_old_files(&download_dir, max_age_secs) {
            warn!("YouTube cleanup error: {}", e);
        }
    }
}

fn cleanup_old_files(dir: &str, max_age_secs: u64) -> anyhow::Result<()> {
    // Create directory if it doesn't exist
    if !std::path::Path::new(dir).exists() {
        fs::create_dir_all(dir)?;
        return Ok(());
    }

    let threshold = SystemTime::now() - Duration::from_secs(max_age_secs);
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                if let Ok(modified) = metadata.modified() {
                    if modified < threshold {
                        if let Err(e) = fs::remove_file(entry.path()) {
                            warn!("Failed to delete old file {:?}: {}", entry.path(), e);
                        } else {
                            debug!("Cleaned up old YouTube cache file: {:?}", entry.path());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
