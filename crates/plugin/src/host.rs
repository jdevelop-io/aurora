//! Host functions exposed to WASM plugins.

/// Host functions that plugins can call.
pub struct HostFunctions {
    // Will be implemented in Phase 4
}

impl HostFunctions {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for HostFunctions {
    fn default() -> Self {
        Self::new()
    }
}
