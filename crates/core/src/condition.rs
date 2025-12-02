//! Conditional execution for beams.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A condition that determines whether a beam should execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Check if a file exists.
    FileExists(PathBuf),

    /// Check if an environment variable is set.
    EnvSet(String),

    /// Check if an environment variable equals a specific value.
    EnvEquals { name: String, value: String },

    /// Run a command and check its exit status.
    Command { run: String, expect_success: bool },

    /// All conditions must be true.
    And(Vec<Condition>),

    /// At least one condition must be true.
    Or(Vec<Condition>),

    /// Negate a condition.
    Not(Box<Condition>),
}

impl Condition {
    /// Creates a file exists condition.
    pub fn file_exists(path: impl Into<PathBuf>) -> Self {
        Self::FileExists(path.into())
    }

    /// Creates an environment variable set condition.
    pub fn env_set(name: impl Into<String>) -> Self {
        Self::EnvSet(name.into())
    }

    /// Creates an environment variable equals condition.
    pub fn env_equals(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self::EnvEquals {
            name: name.into(),
            value: value.into(),
        }
    }

    /// Creates a command condition.
    pub fn command(run: impl Into<String>, expect_success: bool) -> Self {
        Self::Command {
            run: run.into(),
            expect_success,
        }
    }

    /// Combines conditions with AND.
    pub fn and(conditions: Vec<Condition>) -> Self {
        Self::And(conditions)
    }

    /// Combines conditions with OR.
    pub fn or(conditions: Vec<Condition>) -> Self {
        Self::Or(conditions)
    }

    /// Negates a condition.
    pub fn negate(condition: Condition) -> Self {
        Self::Not(Box::new(condition))
    }
}

impl std::ops::Not for Condition {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self::Not(Box::new(self))
    }
}
