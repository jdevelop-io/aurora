//! Cache command implementation.

use std::fs;
use std::path::Path;

use miette::{Result, miette};

use crate::discovery;
use crate::output;

/// Clears the build cache.
pub fn clean(beamfile_path: &Path) -> Result<()> {
    let cache_dir = discovery::cache_dir(beamfile_path);

    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .map_err(|e| miette!("Failed to remove cache directory: {}", e))?;

        output::success("Cache cleared");
    } else {
        output::info("Cache directory does not exist");
    }

    Ok(())
}

/// Shows cache status.
pub fn status(beamfile_path: &Path) -> Result<()> {
    let cache_dir = discovery::cache_dir(beamfile_path);

    if !cache_dir.exists() {
        output::info("No cache exists");
        return Ok(());
    }

    let cache_file = cache_dir.join("cache.json");

    if !cache_file.exists() {
        output::info("Cache is empty");
        return Ok(());
    }

    // Read cache entries
    let content =
        fs::read_to_string(&cache_file).map_err(|e| miette!("Failed to read cache: {}", e))?;

    let entries: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&content).unwrap_or_default();

    println!("Cache status:");
    println!("  Location: {}", cache_dir.display());
    println!("  Entries: {}", entries.len());
    println!();

    for (name, entry) in &entries {
        let timestamp = entry.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);

        let datetime = chrono_lite(timestamp);

        println!("  {} - cached at {}", name, datetime);
    }

    Ok(())
}

/// Simple timestamp formatting (without chrono dependency).
fn chrono_lite(timestamp: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    format!("{:?}", datetime)
}
