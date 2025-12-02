//! Variable definitions for Beamfile.

use serde::{Deserialize, Serialize};

/// A variable that can be used in beam definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name.
    pub name: String,

    /// Default value if not overridden.
    pub default: Option<String>,

    /// Description of the variable.
    pub description: Option<String>,
}

impl Variable {
    /// Creates a new variable with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default: None,
            description: None,
        }
    }

    /// Sets the default value.
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
