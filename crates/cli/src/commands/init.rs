//! Init command implementation.

use std::fs;
use std::path::Path;

use miette::{Result, miette};

use crate::output;

const TEMPLATE: &str = r#"# Aurora Beamfile
# Documentation: https://github.com/jdevelop-io/aurora

# Variables
variable "mode" {
  default = "debug"
  description = "Build mode (debug/release)"
}

# Clean build artifacts
beam "clean" {
  description = "Clean build artifacts"
  run {
    commands = ["echo 'Cleaning...'"]
  }
}

# Build the project
beam "build" {
  description = "Build the project"
  depends_on = ["clean"]

  run {
    commands = [
      "echo 'Building in ${var.mode} mode...'"
    ]
  }
}

# Run tests
beam "test" {
  description = "Run tests"
  depends_on = ["build"]

  run {
    commands = ["echo 'Running tests...'"]
  }
}

# Default beam
default = "build"
"#;

/// Initializes a new Beamfile.
pub fn execute(force: bool) -> Result<()> {
    let beamfile_path = Path::new("Beamfile");

    if beamfile_path.exists() && !force {
        return Err(miette!(
            "Beamfile already exists. Use --force to overwrite."
        ));
    }

    fs::write(beamfile_path, TEMPLATE).map_err(|e| miette!("Failed to write Beamfile: {}", e))?;

    output::success("Created Beamfile");
    output::info("Run 'aurora list' to see available beams");

    Ok(())
}
