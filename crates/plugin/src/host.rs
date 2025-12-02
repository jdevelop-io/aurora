//! Host functions exposed to WASM plugins.
//!
//! These functions are callable by plugins to interact with the Aurora runtime.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// State accessible to plugins through host functions.
#[derive(Debug, Clone, Default)]
pub struct PluginState {
    /// Variables accessible to the plugin.
    variables: Arc<RwLock<HashMap<String, String>>>,

    /// Log messages collected from the plugin.
    logs: Arc<RwLock<Vec<LogEntry>>>,
}

/// A log entry from a plugin.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Log level (0=trace, 1=debug, 2=info, 3=warn, 4=error).
    pub level: i32,
    /// Log message.
    pub message: String,
}

impl PluginState {
    /// Creates a new plugin state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a plugin state with initial variables.
    pub fn with_variables(variables: HashMap<String, String>) -> Self {
        Self {
            variables: Arc::new(RwLock::new(variables)),
            logs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Gets a variable value.
    pub fn get_var(&self, name: &str) -> Option<String> {
        self.variables.read().ok()?.get(name).cloned()
    }

    /// Sets a variable value.
    pub fn set_var(&self, name: &str, value: &str) {
        if let Ok(mut vars) = self.variables.write() {
            vars.insert(name.to_string(), value.to_string());
        }
    }

    /// Gets an environment variable.
    pub fn get_env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// Logs a message.
    pub fn log(&self, level: i32, message: &str) {
        if let Ok(mut logs) = self.logs.write() {
            logs.push(LogEntry {
                level,
                message: message.to_string(),
            });
        }

        // Also print to console based on level
        match level {
            0 => tracing_log(Level::Trace, message),
            1 => tracing_log(Level::Debug, message),
            2 => tracing_log(Level::Info, message),
            3 => tracing_log(Level::Warn, message),
            _ => tracing_log(Level::Error, message),
        }
    }

    /// Gets all log entries.
    pub fn get_logs(&self) -> Vec<LogEntry> {
        self.logs.read().map(|l| l.clone()).unwrap_or_default()
    }

    /// Clears all log entries.
    pub fn clear_logs(&self) {
        if let Ok(mut logs) = self.logs.write() {
            logs.clear();
        }
    }
}

/// Log level for simple logging without tracing crate.
#[derive(Debug, Clone, Copy)]
enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

fn tracing_log(level: Level, message: &str) {
    let prefix = match level {
        Level::Trace => "[TRACE]",
        Level::Debug => "[DEBUG]",
        Level::Info => "[INFO]",
        Level::Warn => "[WARN]",
        Level::Error => "[ERROR]",
    };
    eprintln!("{prefix} [plugin] {message}");
}

/// Host functions that plugins can call.
///
/// This struct holds state and provides methods that are linked
/// into the WASM runtime as host functions.
#[derive(Debug, Clone, Default)]
pub struct HostFunctions {
    state: PluginState,
}

impl HostFunctions {
    /// Creates new host functions with default state.
    pub fn new() -> Self {
        Self {
            state: PluginState::new(),
        }
    }

    /// Creates host functions with the given state.
    pub fn with_state(state: PluginState) -> Self {
        Self { state }
    }

    /// Gets the plugin state.
    pub fn state(&self) -> &PluginState {
        &self.state
    }

    /// Gets a mutable reference to the plugin state.
    pub fn state_mut(&mut self) -> &mut PluginState {
        &mut self.state
    }

    // Host function implementations that will be called from WASM

    /// Logs a message from the plugin.
    ///
    /// level: 0=trace, 1=debug, 2=info, 3=warn, 4=error
    pub fn aurora_log(&self, level: i32, message: &str) {
        self.state.log(level, message);
    }

    /// Gets a variable value.
    pub fn aurora_get_var(&self, name: &str) -> String {
        self.state.get_var(name).unwrap_or_default()
    }

    /// Sets a variable value.
    pub fn aurora_set_var(&self, name: &str, value: &str) {
        self.state.set_var(name, value);
    }

    /// Gets an environment variable.
    pub fn aurora_get_env(&self, name: &str) -> String {
        self.state.get_env(name).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_state_variables() {
        let state = PluginState::new();

        assert!(state.get_var("foo").is_none());

        state.set_var("foo", "bar");
        assert_eq!(state.get_var("foo"), Some("bar".to_string()));

        state.set_var("foo", "baz");
        assert_eq!(state.get_var("foo"), Some("baz".to_string()));
    }

    #[test]
    fn test_plugin_state_with_initial_variables() {
        let mut vars = HashMap::new();
        vars.insert("key1".to_string(), "value1".to_string());
        vars.insert("key2".to_string(), "value2".to_string());

        let state = PluginState::with_variables(vars);

        assert_eq!(state.get_var("key1"), Some("value1".to_string()));
        assert_eq!(state.get_var("key2"), Some("value2".to_string()));
    }

    #[test]
    fn test_plugin_state_logging() {
        let state = PluginState::new();

        state.log(2, "info message");
        state.log(4, "error message");

        let logs = state.get_logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].level, 2);
        assert_eq!(logs[0].message, "info message");
        assert_eq!(logs[1].level, 4);
        assert_eq!(logs[1].message, "error message");

        state.clear_logs();
        assert!(state.get_logs().is_empty());
    }

    #[test]
    fn test_host_functions() {
        let host = HostFunctions::new();

        host.aurora_set_var("test", "value");
        assert_eq!(host.aurora_get_var("test"), "value");
        assert_eq!(host.aurora_get_var("nonexistent"), "");

        host.aurora_log(2, "test log");
        let logs = host.state().get_logs();
        assert_eq!(logs.len(), 1);
    }

    #[test]
    fn test_get_env() {
        let state = PluginState::new();

        // PATH should exist on all systems
        let path = state.get_env("PATH");
        assert!(path.is_some());

        // Non-existent env var
        assert!(state.get_env("AURORA_NONEXISTENT_VAR_12345").is_none());
    }
}
