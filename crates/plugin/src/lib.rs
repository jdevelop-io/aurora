//! Aurora Plugin - WASM plugin system for Aurora.
//!
//! This module provides the infrastructure for loading and executing
//! WASM plugins that can extend Aurora's functionality.
//!
//! # Plugin Architecture
//!
//! Plugins are WebAssembly modules that export specific functions:
//! - `plugin_name() -> (ptr, len)` - Returns the plugin name
//! - `plugin_version() -> (ptr, len)` - Returns the plugin version
//! - `on_beam_start(ptr, len)` - Called before beam execution
//! - `on_beam_complete(ptr, len, success)` - Called after beam execution
//! - `transform_command(ptr, len) -> (ptr, len)` - Transform a command before execution
//! - `alloc(size) -> ptr` - Allocate memory for string passing
//! - `dealloc(ptr, size)` - Free allocated memory
//!
//! # Host Functions
//!
//! Plugins can call these host functions from the "aurora" module:
//! - `aurora_log(level, ptr, len)` - Log a message (level: 0=trace to 4=error)
//! - `aurora_get_var(ptr, len) -> i64` - Get a variable value (returns packed ptr, len)
//! - `aurora_set_var(name_ptr, name_len, val_ptr, val_len)` - Set a variable value
//! - `aurora_get_env(ptr, len) -> i64` - Get an environment variable (returns packed ptr, len)
//!
//! # Plugin Manifest
//!
//! Plugins are distributed as directories containing:
//! - `plugin.json` - Plugin manifest with metadata and capabilities
//! - `plugin.wasm` - The compiled WebAssembly module
//!
//! Example manifest:
//! ```json
//! {
//!   "plugin": {
//!     "name": "my-plugin",
//!     "version": "1.0.0",
//!     "description": "A sample plugin",
//!     "wasm": "plugin.wasm"
//!   },
//!   "capabilities": {
//!     "transform_commands": true,
//!     "beam_hooks": true
//!   }
//! }
//! ```
//!
//! # Example Usage
//!
//! ```ignore
//! use aurora_plugin::{PluginRuntime, PluginState};
//! use std::path::Path;
//!
//! // Create runtime
//! let mut runtime = PluginRuntime::new()?;
//!
//! // Load a plugin from directory
//! let plugin = runtime.load_plugin(Path::new("./my-plugin"))?;
//!
//! // Create an instance with state
//! let state = PluginState::new();
//! let mut instance = runtime.create_instance_with_state(&plugin, state)?;
//!
//! // Call plugin hooks
//! instance.on_beam_start("build")?;
//!
//! // Transform a command
//! if let Some(transformed) = instance.transform_command("cargo build")? {
//!     println!("Transformed: {}", transformed);
//! }
//!
//! instance.on_beam_complete("build", true)?;
//! ```

mod error;
mod host;
mod manifest;
mod runtime;

pub use error::{PluginError, Result};
pub use host::{HostFunctions, LogEntry, PluginState};
pub use manifest::{PluginCapabilities, PluginDependency, PluginManifest, PluginMetadata};
pub use runtime::{Plugin, PluginInstance, PluginRuntime, StoreData};
