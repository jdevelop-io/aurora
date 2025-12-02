//! Validate command implementation.

use std::path::Path;

use miette::{Result, miette};

use crate::output;

/// Validates the Beamfile syntax.
pub fn execute(beamfile_path: &Path) -> Result<()> {
    output::info(&format!("Validating {}...", beamfile_path.display()));

    // Parse Beamfile
    let beamfile = match aurora_parser::parse_file(beamfile_path) {
        Ok(bf) => bf,
        Err(e) => {
            output::error(&format!("Parse error: {}", e));
            return Err(miette!("Validation failed: {}", e));
        }
    };

    // Check for dependency cycles
    match aurora_engine::DependencyGraph::from_beamfile(&beamfile) {
        Ok(_) => {}
        Err(e) => {
            output::error(&format!("Dependency error: {}", e));
            return Err(miette!("Validation failed: {}", e));
        }
    }

    // Check for undefined dependencies
    for (name, beam) in &beamfile.beams {
        for dep in &beam.depends_on {
            if beamfile.get_beam(dep).is_none() {
                output::error(&format!(
                    "Beam '{}' depends on undefined beam '{}'",
                    name, dep
                ));
                return Err(miette!("Validation failed: undefined dependency"));
            }
        }
    }

    // Check default beam exists
    if let Some(ref default) = beamfile.default_beam {
        if beamfile.get_beam(default).is_none() {
            output::error(&format!("Default beam '{}' does not exist", default));
            return Err(miette!("Validation failed: invalid default beam"));
        }
    }

    output::success(&format!(
        "Beamfile is valid ({} beams, {} variables)",
        beamfile.beams.len(),
        beamfile.variables.len()
    ));

    Ok(())
}
