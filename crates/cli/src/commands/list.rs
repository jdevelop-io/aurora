//! List command implementation.

use std::path::Path;

use console::style;
use miette::{Result, miette};

/// Lists all available beams.
pub fn execute(beamfile_path: &Path, detailed: bool) -> Result<()> {
    let beamfile = aurora_parser::parse_file(beamfile_path)
        .map_err(|e| miette!("Failed to parse Beamfile: {}", e))?;

    println!("{}", style("Available beams:").bold());
    println!();

    let mut beam_names: Vec<_> = beamfile.beam_names();
    beam_names.sort();

    for name in beam_names {
        let beam = beamfile.get_beam(name).unwrap();
        let is_default = beamfile.default_beam.as_deref() == Some(name);

        if detailed {
            print!("  {}", style(name).cyan().bold());

            if is_default {
                print!(" {}", style("(default)").yellow());
            }

            println!();

            if let Some(desc) = &beam.description {
                println!("    {}", style(desc).dim());
            }

            if !beam.depends_on.is_empty() {
                println!(
                    "    Dependencies: {}",
                    style(beam.depends_on.join(", ")).dim()
                );
            }

            println!();
        } else {
            print!("  {}", name);

            if is_default {
                print!(" {}", style("(default)").yellow());
            }

            if let Some(desc) = &beam.description {
                print!(" - {}", style(desc).dim());
            }

            println!();
        }
    }

    Ok(())
}
