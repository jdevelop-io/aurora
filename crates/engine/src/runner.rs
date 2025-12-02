//! Cross-platform command execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use aurora_core::{AuroraError, Result, RunBlock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;

/// Callback for command output.
pub type OutputCallback = Arc<dyn Fn(&str, bool) + Send + Sync>;

/// Executes shell commands.
#[derive(Clone)]
pub struct CommandRunner {
    /// Shell to use for execution.
    shell: Shell,

    /// Default working directory.
    working_dir: PathBuf,

    /// Default environment variables.
    env: HashMap<String, String>,

    /// Optional callback for streaming output.
    output_callback: Option<OutputCallback>,
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
            output_callback: None,
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

    /// Sets a callback for streaming output.
    pub fn with_output_callback(mut self, callback: OutputCallback) -> Self {
        self.output_callback = Some(callback);
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

        if self.output_callback.is_some() {
            self.execute_with_streaming(command, &program, &args, working_dir, env)
                .await
        } else {
            self.execute_buffered(&program, &args, working_dir, env, command)
                .await
        }
    }

    /// Executes a command with buffered output (original behavior).
    async fn execute_buffered(
        &self,
        program: &str,
        args: &[String],
        working_dir: &Path,
        env: &HashMap<String, String>,
        command: &str,
    ) -> Result<CommandResult> {
        let output = TokioCommand::new(program)
            .args(args)
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

    /// Executes a command with streaming output.
    async fn execute_with_streaming(
        &self,
        command: &str,
        program: &str,
        args: &[String],
        working_dir: &Path,
        env: &HashMap<String, String>,
    ) -> Result<CommandResult> {
        let mut child = TokioCommand::new(program)
            .args(args)
            .current_dir(working_dir)
            .envs(env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AuroraError::CommandFailed {
                command: command.to_string(),
                exit_code: None,
                stderr: Some(e.to_string()),
            })?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let callback = self.output_callback.clone().unwrap();

        // Create channels to collect output
        let (stdout_tx, _stdout_rx) = mpsc::channel::<String>(100);
        let (stderr_tx, _stderr_rx) = mpsc::channel::<String>(100);

        // Spawn stdout reader
        let callback_stdout = callback.clone();
        let stdout_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut collected = Vec::new();

            while let Ok(Some(line)) = lines.next_line().await {
                callback_stdout(&line, false);
                collected.push(line.clone());
                let _ = stdout_tx.send(line).await;
            }

            collected.join("\n")
        });

        // Spawn stderr reader
        let callback_stderr = callback.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut collected = Vec::new();

            while let Ok(Some(line)) = lines.next_line().await {
                callback_stderr(&line, true);
                collected.push(line.clone());
                let _ = stderr_tx.send(line).await;
            }

            collected.join("\n")
        });

        // Wait for the process to complete
        let status = child.wait().await.map_err(|e| AuroraError::CommandFailed {
            command: command.to_string(),
            exit_code: None,
            stderr: Some(e.to_string()),
        })?;

        // Collect remaining output from channels
        drop(_stdout_rx);
        drop(_stderr_rx);

        // Wait for readers to complete
        let stdout_output = stdout_handle.await.unwrap_or_default();
        let stderr_output = stderr_handle.await.unwrap_or_default();

        Ok(CommandResult {
            exit_code: status.code().unwrap_or(-1),
            stdout: stdout_output,
            stderr: stderr_output,
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[tokio::test]
    async fn test_streaming_output() {
        let line_count = Arc::new(AtomicUsize::new(0));
        let line_count_clone = line_count.clone();

        let callback: OutputCallback = Arc::new(move |_line, _is_stderr| {
            line_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let runner = CommandRunner::new(".").with_output_callback(callback);

        let result = runner
            .execute_command(
                "echo line1 && echo line2 && echo line3",
                Path::new("."),
                &HashMap::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(line_count.load(Ordering::SeqCst) >= 3);
    }
}
