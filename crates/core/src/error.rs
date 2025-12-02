//! Error types for Aurora.

use std::path::PathBuf;

use thiserror::Error;

/// Result type alias for Aurora operations.
pub type Result<T> = std::result::Result<T, AuroraError>;

/// Main error type for Aurora.
#[derive(Debug, Error)]
pub enum AuroraError {
    #[error("Beamfile not found in {0} or any parent directory")]
    BeamfileNotFound(PathBuf),

    #[error("Failed to read file: {path}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Parse error: {message}")]
    Parse { message: String, span: Option<Span> },

    #[error("Beam '{0}' not found")]
    BeamNotFound(String),

    #[error("Dependency cycle detected: {0}")]
    CycleDetected(String),

    #[error("Variable '{0}' not found")]
    VariableNotFound(String),

    #[error("Condition evaluation failed: {0}")]
    ConditionFailed(String),

    #[error("Command execution failed: {command}")]
    CommandFailed {
        command: String,
        exit_code: Option<i32>,
        stderr: Option<String>,
    },

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Source span for error reporting.
#[derive(Debug, Clone)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}
