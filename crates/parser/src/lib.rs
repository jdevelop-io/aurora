//! Aurora Parser - Beamfile DSL parser using nom combinators.

mod ast;
mod combinators;
mod error;
mod lexer;
mod parser;

pub use error::ParseError;
pub use parser::parse_beamfile;

use std::path::Path;

use aurora_core::{Beamfile, Result};

/// Parses a Beamfile from the given path.
pub fn parse_file(path: &Path) -> Result<Beamfile> {
    let content =
        std::fs::read_to_string(path).map_err(|e| aurora_core::AuroraError::FileRead {
            path: path.to_path_buf(),
            source: e,
        })?;

    parse_beamfile(&content, path)
}

/// Parses a Beamfile from a string.
pub fn parse_str(content: &str) -> Result<Beamfile> {
    parse_beamfile(content, Path::new("<string>"))
}
