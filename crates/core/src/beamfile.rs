//! Beamfile structure representing the parsed configuration.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::beam::Beam;
use crate::variable::Variable;

/// The root structure representing a parsed Beamfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beamfile {
    /// Path to the Beamfile.
    pub path: PathBuf,

    /// Variables defined in the Beamfile.
    pub variables: HashMap<String, Variable>,

    /// Beams (targets) defined in the Beamfile.
    pub beams: HashMap<String, Beam>,

    /// Default beam to run when no target is specified.
    pub default_beam: Option<String>,
}

impl Beamfile {
    /// Creates a new empty Beamfile.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            variables: HashMap::new(),
            beams: HashMap::new(),
            default_beam: None,
        }
    }

    /// Adds a variable to the Beamfile.
    pub fn add_variable(&mut self, variable: Variable) {
        self.variables.insert(variable.name.clone(), variable);
    }

    /// Adds a beam to the Beamfile.
    pub fn add_beam(&mut self, beam: Beam) {
        self.beams.insert(beam.name.clone(), beam);
    }

    /// Sets the default beam.
    pub fn set_default_beam(&mut self, name: impl Into<String>) {
        self.default_beam = Some(name.into());
    }

    /// Gets a beam by name.
    pub fn get_beam(&self, name: &str) -> Option<&Beam> {
        self.beams.get(name)
    }

    /// Gets a variable by name.
    pub fn get_variable(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name)
    }

    /// Returns all beam names.
    pub fn beam_names(&self) -> Vec<&str> {
        self.beams.keys().map(|s| s.as_str()).collect()
    }

    /// Returns all variable names.
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for Beamfile {
    fn default() -> Self {
        Self::new("")
    }
}
