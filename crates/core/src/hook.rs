//! Pre and post hooks for beams.

use serde::{Deserialize, Serialize};

/// A hook that runs before or after a beam's main commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    /// Commands to execute.
    pub commands: Vec<String>,

    /// Optional shell to use (overrides beam/global setting).
    pub shell: Option<String>,

    /// Optional working directory.
    pub working_dir: Option<String>,

    /// Whether to fail the beam if the hook fails.
    pub fail_on_error: bool,
}

impl Hook {
    /// Creates a new hook with the given commands.
    pub fn new(commands: Vec<String>) -> Self {
        Self {
            commands,
            shell: None,
            working_dir: None,
            fail_on_error: true,
        }
    }

    /// Sets the shell to use.
    pub fn with_shell(mut self, shell: impl Into<String>) -> Self {
        self.shell = Some(shell.into());
        self
    }

    /// Sets the working directory.
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Sets whether to fail on error.
    pub fn fail_on_error(mut self, fail: bool) -> Self {
        self.fail_on_error = fail;
        self
    }
}

impl Default for Hook {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            shell: None,
            working_dir: None,
            fail_on_error: true,
        }
    }
}
