//! Run command implementation.

use std::path::Path;

use miette::{Result, miette};

use crate::discovery;
use crate::output;

/// Executes a beam.
pub async fn execute(
    beamfile_path: &Path,
    target: &str,
    parallel: usize,
    dry_run: bool,
    use_cache: bool,
) -> Result<()> {
    // Parse Beamfile
    let beamfile = aurora_parser::parse_file(beamfile_path)
        .map_err(|e| miette!("Failed to parse Beamfile: {}", e))?;

    // Verify target exists
    if beamfile.get_beam(target).is_none() {
        return Err(miette!("Beam '{}' not found", target));
    }

    let working_dir = discovery::working_dir(beamfile_path);
    let cache_dir = discovery::cache_dir(beamfile_path);

    // Create executor
    let mut executor = aurora_engine::Executor::new(beamfile, &working_dir, &cache_dir)
        .map_err(|e| miette!("Failed to create executor: {}", e))?
        .with_cache(use_cache)
        .with_dry_run(dry_run);

    if parallel > 0 {
        executor = executor.with_max_parallelism(parallel);
    }

    if dry_run {
        output::info("Dry run mode - no commands will be executed");
    }

    output::info(&format!("Executing beam: {}", target));

    // Execute
    let report = executor
        .execute(target)
        .await
        .map_err(|e| miette!("Execution failed: {}", e))?;

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

    output::summary(
        report.executed.len(),
        report.skipped.len(),
        report.failed.len(),
        report.duration_ms,
    );

    if !report.failed.is_empty() {
        return Err(miette!("Execution failed"));
    }

    Ok(())
}
