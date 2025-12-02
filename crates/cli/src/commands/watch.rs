//! Watch command implementation.
//!
//! Watches input files and re-executes beams when they change.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{Result, miette};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::discovery;
use crate::output;

/// Minimum interval between rebuilds (debounce).
const DEBOUNCE_MS: u64 = 300;

/// Watch mode configuration.
pub struct WatchConfig {
    /// Beamfile path.
    pub beamfile_path: PathBuf,
    /// Target beam to execute.
    pub target: String,
    /// Maximum parallel jobs.
    pub parallel: usize,
    /// Whether to use cache.
    pub use_cache: bool,
    /// Whether to clear screen before each run.
    pub clear_screen: bool,
}

/// Executes watch mode.
pub async fn execute(
    beamfile_path: &Path,
    target: &str,
    parallel: usize,
    use_cache: bool,
    clear_screen: bool,
) -> Result<()> {
    let config = WatchConfig {
        beamfile_path: beamfile_path.to_path_buf(),
        target: target.to_string(),
        parallel,
        use_cache,
        clear_screen,
    };

    run_watch_loop(config).await
}

/// Main watch loop.
async fn run_watch_loop(config: WatchConfig) -> Result<()> {
    // Parse initial beamfile
    let beamfile = aurora_parser::parse_file(&config.beamfile_path)
        .map_err(|e| miette!("Failed to parse Beamfile: {}", e))?;

    // Verify target exists
    let beam = beamfile
        .get_beam(&config.target)
        .ok_or_else(|| miette!("Beam '{}' not found", config.target))?;

    // Collect all input patterns
    let inputs: Vec<PathBuf> = beam.inputs.clone();
    let working_dir = discovery::working_dir(&config.beamfile_path);

    // If no inputs defined, watch the Beamfile itself
    let watch_paths = if inputs.is_empty() {
        output::warning("No inputs defined for this beam, watching Beamfile only");
        vec![config.beamfile_path.clone()]
    } else {
        collect_watch_paths(&inputs, &working_dir)?
    };

    println!(
        "\n{} Watching {} for changes...\n",
        style("👁").cyan(),
        style(&config.target).cyan().bold()
    );

    for path in &watch_paths {
        println!("  {} {}", style("•").dim(), style(path.display()).dim());
    }
    println!();

    // Create channel for file events
    let (tx, mut rx) = mpsc::channel::<PathBuf>(100);

    // Create watcher
    let tx_clone = tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: std::result::Result<Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = tx_clone.blocking_send(path);
                }
            }
        },
        Config::default().with_poll_interval(Duration::from_millis(200)),
    )
    .map_err(|e| miette!("Failed to create file watcher: {}", e))?;

    // Add paths to watcher
    for path in &watch_paths {
        let mode = if path.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        watcher
            .watch(path, mode)
            .map_err(|e| miette!("Failed to watch {}: {}", path.display(), e))?;
    }

    // Also watch the Beamfile for changes
    watcher
        .watch(&config.beamfile_path, RecursiveMode::NonRecursive)
        .map_err(|e| miette!("Failed to watch Beamfile: {}", e))?;

    // Run initial build
    run_build(&config).await;

    // Create spinner for waiting state
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("Invalid spinner template"),
    );
    spinner.set_message("Waiting for changes...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    let mut last_rebuild = Instant::now();

    while let Some(changed_path) = rx.recv().await {
        // Debounce rapid changes
        let elapsed = last_rebuild.elapsed();
        if elapsed < Duration::from_millis(DEBOUNCE_MS) {
            continue;
        }

        // Drain any additional pending events
        while rx.try_recv().is_ok() {}

        spinner.finish_and_clear();

        println!(
            "\n{} File changed: {}\n",
            style("↻").yellow().bold(),
            style(changed_path.display()).yellow()
        );

        // Clear screen if requested
        if config.clear_screen {
            print!("\x1B[2J\x1B[1;1H");
        }

        // Re-run build
        run_build(&config).await;

        last_rebuild = Instant::now();

        // Restart spinner
        spinner.set_message("Waiting for changes...");
        spinner.enable_steady_tick(Duration::from_millis(100));
    }

    Ok(())
}

/// Collects paths to watch based on input patterns.
fn collect_watch_paths(patterns: &[PathBuf], working_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = HashSet::new();

    for pattern in patterns {
        let pattern_str = pattern.to_string_lossy();

        // For glob patterns, watch the parent directory
        if pattern_str.contains('*') {
            // Find the static prefix before any glob
            let parts: Vec<&str> = pattern_str.split('/').collect();
            let mut static_parts = Vec::new();

            for part in parts {
                if part.contains('*') || part.contains('?') || part.contains('[') {
                    break;
                }
                static_parts.push(part);
            }

            let watch_path = if static_parts.is_empty() {
                working_dir.to_path_buf()
            } else {
                working_dir.join(static_parts.join("/"))
            };

            if watch_path.exists() {
                paths.insert(watch_path);
            }
        } else {
            // Direct file or directory path
            let full_path = working_dir.join(pattern);
            if full_path.exists() {
                paths.insert(full_path);
            }
        }
    }

    // If no valid paths found, default to working directory
    if paths.is_empty() {
        paths.insert(working_dir.to_path_buf());
    }

    Ok(paths.into_iter().collect())
}

/// Runs a single build.
async fn run_build(config: &WatchConfig) {
    let start = Instant::now();

    // Re-parse Beamfile (it might have changed)
    let beamfile = match aurora_parser::parse_file(&config.beamfile_path) {
        Ok(bf) => bf,
        Err(e) => {
            output::error(&format!("Failed to parse Beamfile: {}", e));
            return;
        }
    };

    let working_dir = discovery::working_dir(&config.beamfile_path);
    let cache_dir = discovery::cache_dir(&config.beamfile_path);

    // Create executor
    let executor = match aurora_engine::Executor::new(beamfile, &working_dir, &cache_dir) {
        Ok(e) => e,
        Err(e) => {
            output::error(&format!("Failed to create executor: {}", e));
            return;
        }
    };

    let mut executor = executor.with_cache(config.use_cache);

    if config.parallel > 0 {
        executor = executor.with_max_parallelism(config.parallel);
    }

    // Create progress bar for execution
    let progress = ProgressBar::new_spinner();
    progress.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("Invalid progress template"),
    );
    progress.set_message(format!("Building {}...", config.target));
    progress.enable_steady_tick(Duration::from_millis(80));

    // Execute
    let result = executor.execute(&config.target).await;

    progress.finish_and_clear();

    match result {
        Ok(report) => {
            // Print results
            for beam in &report.executed {
                output::beam_completed(beam, 0);
            }

            for beam in &report.skipped {
                output::beam_skipped(beam);
            }

            for (beam, error) in &report.failed {
                output::beam_failed(beam, error);
            }

            let duration = start.elapsed().as_millis() as u64;

            if report.failed.is_empty() {
                println!(
                    "\n{} Build completed in {}ms\n",
                    style("✓").green().bold(),
                    duration
                );
            } else {
                println!(
                    "\n{} Build failed in {}ms\n",
                    style("✗").red().bold(),
                    duration
                );
            }
        }
        Err(e) => {
            output::error(&format!("Execution failed: {}", e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_watch_paths_glob() {
        let tempdir = tempfile::tempdir().unwrap();
        let working_dir = tempdir.path();

        // Create src directory
        std::fs::create_dir(working_dir.join("src")).unwrap();

        let patterns = vec![PathBuf::from("src/**/*.rs")];
        let paths = collect_watch_paths(&patterns, working_dir).unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("src"));
    }

    #[test]
    fn test_collect_watch_paths_direct() {
        let tempdir = tempfile::tempdir().unwrap();
        let working_dir = tempdir.path();

        // Create a file
        std::fs::write(working_dir.join("file.txt"), "test").unwrap();

        let patterns = vec![PathBuf::from("file.txt")];
        let paths = collect_watch_paths(&patterns, working_dir).unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("file.txt"));
    }

    #[test]
    fn test_collect_watch_paths_fallback() {
        let tempdir = tempfile::tempdir().unwrap();
        let working_dir = tempdir.path();

        // Non-existent patterns should fall back to working dir
        let patterns = vec![PathBuf::from("nonexistent/**/*.rs")];
        let paths = collect_watch_paths(&patterns, working_dir).unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], working_dir);
    }
}
