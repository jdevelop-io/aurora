//! Cross-platform command execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use aurora_core::{AuroraError, Result, RunBlock};
use tokio::process::Command as TokioCommand;

/// Executes shell commands.
pub struct CommandRunner {
    /// Shell to use for execution.
    shell: Shell,

    /// Default working directory.
    working_dir: PathBuf,

    /// Default environment variables.
    env: HashMap<String, String>,
}

/// Shell configuration.
#[derive(Debug, Clone)]
pub enum Shell {
    /// Unix shell (sh, bash, zsh, etc.)
    #[cfg(unix)]
    Unix { path: PathBuf },

    /// Windows PowerShell
    #[cfg(windows)]
    PowerShell,

    /// Windows cmd.exe
    #[cfg(windows)]
    Cmd,
}

/// Result of command execution.
#[derive(Debug)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandRunner {
    /// Creates a new command runner with default shell.
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            shell: Self::default_shell(),
            working_dir: working_dir.into(),
            env: HashMap::new(),
        }
    }

    /// Detects the default shell for the current platform.
    fn default_shell() -> Shell {
        #[cfg(unix)]
        {
            // Try to find a suitable shell
            let shells = ["/bin/bash", "/bin/sh", "/usr/bin/bash", "/usr/bin/sh"];

            for shell_path in shells {
                if Path::new(shell_path).exists() {
                    return Shell::Unix {
                        path: PathBuf::from(shell_path),
                    };
                }
            }

            // Fallback to sh
            Shell::Unix {
                path: PathBuf::from("/bin/sh"),
            }
        }

        #[cfg(windows)]
        {
            // Prefer PowerShell on Windows
            Shell::PowerShell
        }
    }

    /// Sets the shell to use.
    pub fn with_shell(mut self, shell: Shell) -> Self {
        self.shell = shell;
        self
    }

    /// Sets the shell from a string path.
    pub fn with_shell_path(mut self, path: impl Into<PathBuf>) -> Self {
        #[cfg(unix)]
        {
            self.shell = Shell::Unix { path: path.into() };
        }

        #[cfg(windows)]
        {
            let path_str = path.into();
            let path_lower = path_str.to_string_lossy().to_lowercase();
            if path_lower.contains("powershell") {
                self.shell = Shell::PowerShell;
            } else {
                self.shell = Shell::Cmd;
            }
        }

        self
    }

    /// Adds environment variables.
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env.extend(env);
        self
    }

    /// Executes a run block.
    pub async fn execute_run_block(
        &self,
        run: &RunBlock,
        extra_env: &HashMap<String, String>,
    ) -> Result<()> {
        let working_dir = run
            .working_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.working_dir.clone());

        let mut merged_env = self.env.clone();
        merged_env.extend(extra_env.clone());

        for cmd in &run.commands {
            let result = self
                .execute_command(&cmd.command, &working_dir, &merged_env)
                .await?;

            if result.exit_code != 0 && run.fail_fast {
                return Err(AuroraError::CommandFailed {
                    command: cmd.command.clone(),
                    exit_code: Some(result.exit_code),
                    stderr: Some(result.stderr),
                });
            }
        }

        Ok(())
    }

    /// Executes a single command.
    pub async fn execute_command(
        &self,
        command: &str,
        working_dir: &Path,
        env: &HashMap<String, String>,
    ) -> Result<CommandResult> {
        let (program, args) = self.shell_args(command);

        let output = TokioCommand::new(&program)
            .args(&args)
            .current_dir(working_dir)
            .envs(env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AuroraError::CommandFailed {
                command: command.to_string(),
                exit_code: None,
                stderr: Some(e.to_string()),
            })?;

        Ok(CommandResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Returns the program and arguments for executing a command in the shell.
    fn shell_args(&self, command: &str) -> (String, Vec<String>) {
        match &self.shell {
            #[cfg(unix)]
            Shell::Unix { path } => (
                path.to_string_lossy().to_string(),
                vec!["-c".to_string(), command.to_string()],
            ),

            #[cfg(windows)]
            Shell::PowerShell => (
                "powershell.exe".to_string(),
                vec![
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-Command".to_string(),
                    command.to_string(),
                ],
            ),

            #[cfg(windows)]
            Shell::Cmd => (
                "cmd.exe".to_string(),
                vec!["/C".to_string(), command.to_string()],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_echo() {
        let runner = CommandRunner::new(".");
        let result = runner
            .execute_command("echo hello", Path::new("."), &HashMap::new())
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.trim().contains("hello"));
    }

    #[tokio::test]
    async fn test_execute_failing_command() {
        let runner = CommandRunner::new(".");
        let result = runner
            .execute_command("exit 42", Path::new("."), &HashMap::new())
            .await
            .unwrap();

        assert_eq!(result.exit_code, 42);
    }
}
