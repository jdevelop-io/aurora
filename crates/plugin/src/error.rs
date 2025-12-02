//! Plugin error types.

use std::path::PathBuf;

use thiserror::Error;

/// Error type for plugin operations.
#[derive(Debug, Error)]
pub enum PluginError {
    /// Failed to load plugin file.
    #[error("Failed to load plugin from {path}: {reason}")]
    LoadError { path: PathBuf, reason: String },

    /// Plugin file not found.
    #[error("Plugin not found: {0}")]
    NotFound(PathBuf),

    /// Invalid plugin format.
    #[error("Invalid plugin format: {0}")]
    InvalidFormat(String),

    /// Plugin initialization failed.
    #[error("Plugin initialization failed: {0}")]
    InitError(String),

    /// Plugin execution error.
    #[error("Plugin execution error: {0}")]
    ExecutionError(String),

    /// Plugin function not found.
    #[error("Plugin function '{0}' not found")]
    FunctionNotFound(String),

    /// Plugin returned an error.
    #[error("Plugin error: {0}")]
    PluginError(String),

    /// Manifest parsing error.
    #[error("Failed to parse plugin manifest: {0}")]
    ManifestError(String),

    /// WASM runtime error.
    #[error("WASM runtime error: {0}")]
    WasmError(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<wasmtime::Error> for PluginError {
    fn from(err: wasmtime::Error) -> Self {
        PluginError::WasmError(err.to_string())
    }
}

/// Result type for plugin operations.
pub type Result<T> = std::result::Result<T, PluginError>;
