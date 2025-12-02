//! Aurora Plugin - WASM plugin system for Aurora.
//!
//! This module provides the infrastructure for loading and executing
//! WASM plugins that can extend Aurora's functionality.

mod host;
mod runtime;

pub use host::HostFunctions;
pub use runtime::PluginRuntime;

// Plugin system will be implemented in Phase 4
// For now, provide stub implementations
