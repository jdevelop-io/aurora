//! Graph command implementation.

use std::path::Path;

use miette::{Result, miette};

/// Shows the dependency graph.
pub fn execute(beamfile_path: &Path, target: Option<&str>, format: &str) -> Result<()> {
    let beamfile = aurora_parser::parse_file(beamfile_path)
        .map_err(|e| miette!("Failed to parse Beamfile: {}", e))?;

    let _dag = aurora_engine::DependencyGraph::from_beamfile(&beamfile)
        .map_err(|e| miette!("Failed to build dependency graph: {}", e))?;

    match format {
        "ascii" => print_ascii(&beamfile, target),
        "dot" => print_dot(&beamfile, target),
        _ => return Err(miette!("Unknown format: {}. Use 'ascii' or 'dot'", format)),
    }

    Ok(())
}

/// Prints an ASCII representation of the dependency graph.
fn print_ascii(beamfile: &aurora_core::Beamfile, target: Option<&str>) {
    println!("Dependency Graph:");
    println!();

    let beams: Vec<_> = match target {
        Some(t) => vec![t],
        None => beamfile.beam_names(),
    };

    for name in beams {
        if let Some(beam) = beamfile.get_beam(name) {
            print_beam_ascii(name, &beam.depends_on, 0);
        }
    }
}

/// Recursively prints a beam and its dependencies.
fn print_beam_ascii(name: &str, deps: &[String], depth: usize) {
    let indent = "  ".repeat(depth);
    let prefix = if depth == 0 { "●" } else { "├─" };

    println!("{}{} {}", indent, prefix, name);

    // Note: In a full implementation, we would recursively show dependencies
    // For now, just show direct dependencies
    for dep in deps {
        println!("{}  └─ {}", indent, dep);
    }
}

/// Prints a DOT format representation for Graphviz.
fn print_dot(beamfile: &aurora_core::Beamfile, _target: Option<&str>) {
    println!("digraph aurora {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box];");
    println!();

    for (name, beam) in &beamfile.beams {
        // Node
        let label = match &beam.description {
            Some(desc) => format!("{}\\n{}", name, desc),
            None => name.clone(),
        };
        println!("  \"{}\" [label=\"{}\"];", name, label);

        // Edges
        for dep in &beam.depends_on {
            println!("  \"{}\" -> \"{}\";", dep, name);
        }
    }

    println!("}}");
}
