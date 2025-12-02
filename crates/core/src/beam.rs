//! Beam (target) definition.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::condition::Condition;
use crate::hook::Hook;

/// A beam represents a build target or task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beam {
    /// Beam name (identifier).
    pub name: String,

    /// Human-readable description.
    pub description: Option<String>,

    /// List of beam names this beam depends on.
    pub depends_on: Vec<String>,

    /// Condition that must be true for the beam to execute.
    pub condition: Option<Condition>,

    /// Environment variables to set for this beam.
    pub env: HashMap<String, String>,

    /// Pre-execution hooks.
    pub pre_hooks: Vec<Hook>,

    /// Main run block with commands.
    pub run: Option<RunBlock>,

    /// Post-execution hooks.
    pub post_hooks: Vec<Hook>,

    /// Input files (for cache invalidation).
    pub inputs: Vec<PathBuf>,

    /// Output files (for cache validation).
    pub outputs: Vec<PathBuf>,
}

/// The main run block containing commands to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunBlock {
    /// Commands to execute.
    pub commands: Vec<Command>,

    /// Shell to use for execution.
    pub shell: Option<String>,

    /// Working directory for commands.
    pub working_dir: Option<String>,

    /// Stop execution on first command failure.
    pub fail_fast: bool,
}

/// A command to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// The command string to execute.
    pub command: String,

    /// Optional description for the command.
    pub description: Option<String>,
}

impl Beam {
    /// Creates a new beam with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            depends_on: Vec::new(),
            condition: None,
            env: HashMap::new(),
            pre_hooks: Vec::new(),
            run: None,
            post_hooks: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds dependencies.
    pub fn with_depends_on(mut self, deps: Vec<String>) -> Self {
        self.depends_on = deps;
        self
    }

    /// Sets the condition.
    pub fn with_condition(mut self, condition: Condition) -> Self {
        self.condition = Some(condition);
        self
    }

    /// Adds environment variables.
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Sets the run block.
    pub fn with_run(mut self, run: RunBlock) -> Self {
        self.run = Some(run);
        self
    }

    /// Adds pre-hooks.
    pub fn with_pre_hooks(mut self, hooks: Vec<Hook>) -> Self {
        self.pre_hooks = hooks;
        self
    }

    /// Adds post-hooks.
    pub fn with_post_hooks(mut self, hooks: Vec<Hook>) -> Self {
        self.post_hooks = hooks;
        self
    }

    /// Sets input files.
    pub fn with_inputs(mut self, inputs: Vec<PathBuf>) -> Self {
        self.inputs = inputs;
        self
    }

    /// Sets output files.
    pub fn with_outputs(mut self, outputs: Vec<PathBuf>) -> Self {
        self.outputs = outputs;
        self
    }
}

impl RunBlock {
    /// Creates a new run block with the given commands.
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            shell: None,
            working_dir: None,
            fail_fast: true,
        }
    }

    /// Creates a run block from string commands.
    pub fn from_strings(commands: Vec<String>) -> Self {
        Self::new(commands.into_iter().map(Command::new).collect())
    }

    /// Sets the shell.
    pub fn with_shell(mut self, shell: impl Into<String>) -> Self {
        self.shell = Some(shell.into());
        self
    }

    /// Sets the working directory.
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Sets fail fast mode.
    pub fn with_fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }
}

impl Command {
    /// Creates a new command.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            description: None,
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
