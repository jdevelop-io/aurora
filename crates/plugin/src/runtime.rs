//! WASM plugin runtime using wasmtime.

use std::path::Path;

use aurora_core::Result;
use thiserror::Error;

/// Error type for plugin operations.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Failed to load plugin: {0}")]
    LoadError(String),

    #[error("Plugin execution error: {0}")]
    ExecutionError(String),
}

/// Runtime for executing WASM plugins.
#[derive(Default)]
pub struct PluginRuntime {
    // Will be implemented in Phase 4 with wasmtime
}

impl PluginRuntime {
    /// Creates a new plugin runtime.
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Loads a plugin from a WASM file.
    pub fn load_plugin(&mut self, _path: &Path) -> std::result::Result<(), PluginError> {
        // Stub - will be implemented in Phase 4
        Err(PluginError::LoadError(
            "Plugin system not yet implemented".to_string(),
        ))
    }
}
