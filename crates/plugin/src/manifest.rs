//! Plugin manifest for metadata and configuration.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{PluginError, Result};

/// Plugin manifest containing metadata and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin metadata.
    pub plugin: PluginMetadata,

    /// Plugin capabilities.
    #[serde(default)]
    pub capabilities: PluginCapabilities,

    /// Plugin dependencies.
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
}

/// Plugin metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name.
    pub name: String,

    /// Plugin version.
    pub version: String,

    /// Plugin description.
    #[serde(default)]
    pub description: Option<String>,

    /// Plugin author.
    #[serde(default)]
    pub author: Option<String>,

    /// Plugin license.
    #[serde(default)]
    pub license: Option<String>,

    /// Plugin homepage.
    #[serde(default)]
    pub homepage: Option<String>,

    /// Path to the WASM file (relative to manifest).
    #[serde(default = "default_wasm_path")]
    pub wasm: String,
}

fn default_wasm_path() -> String {
    "plugin.wasm".to_string()
}

/// Plugin capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// Can transform commands before execution.
    #[serde(default)]
    pub transform_commands: bool,

    /// Can hook into beam lifecycle.
    #[serde(default)]
    pub beam_hooks: bool,

    /// Can access environment variables.
    #[serde(default)]
    pub env_access: bool,

    /// Can access file system (read-only).
    #[serde(default)]
    pub fs_read: bool,

    /// Can access file system (write).
    #[serde(default)]
    pub fs_write: bool,

    /// Can make network requests.
    #[serde(default)]
    pub network: bool,
}

/// Plugin dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// Dependency name.
    pub name: String,

    /// Required version.
    pub version: String,
}

impl PluginManifest {
    /// Loads a manifest from a JSON file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Parses a manifest from JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| PluginError::ManifestError(e.to_string()))
    }

    /// Converts the manifest to JSON string.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| PluginError::ManifestError(e.to_string()))
    }

    /// Creates a minimal manifest with just name and version.
    pub fn minimal(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            plugin: PluginMetadata {
                name: name.into(),
                version: version.into(),
                description: None,
                author: None,
                license: None,
                homepage: None,
                wasm: default_wasm_path(),
            },
            capabilities: PluginCapabilities::default(),
            dependencies: Vec::new(),
        }
    }
}

impl PluginMetadata {
    /// Creates new plugin metadata.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: None,
            author: None,
            license: None,
            homepage: None,
            wasm: default_wasm_path(),
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the author.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest() {
        let json = r#"{
            "plugin": {
                "name": "test-plugin",
                "version": "1.0.0",
                "description": "A test plugin"
            },
            "capabilities": {
                "transform_commands": true,
                "beam_hooks": true
            }
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert_eq!(manifest.plugin.name, "test-plugin");
        assert_eq!(manifest.plugin.version, "1.0.0");
        assert!(manifest.capabilities.transform_commands);
        assert!(manifest.capabilities.beam_hooks);
        assert!(!manifest.capabilities.network);
    }

    #[test]
    fn test_minimal_manifest() {
        let manifest = PluginManifest::minimal("my-plugin", "0.1.0");
        assert_eq!(manifest.plugin.name, "my-plugin");
        assert_eq!(manifest.plugin.version, "0.1.0");
        assert_eq!(manifest.plugin.wasm, "plugin.wasm");
    }

    #[test]
    fn test_serialize_manifest() {
        let manifest = PluginManifest::minimal("test", "1.0.0");
        let json = manifest.to_json().unwrap();
        assert!(json.contains("\"name\": \"test\""));
        assert!(json.contains("\"version\": \"1.0.0\""));
    }
}
