use anyhow::{bail, Result};
use async_trait::async_trait;
use aurora_executor_api::{pump_child, ExecutionInput, ExecutionOutput, Executor};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Kills the child's process group with SIGKILL when dropped while still armed.
/// `kill_on_drop` only reaches the direct `sh`; a command that backgrounds a
/// job (`... &`) or spawns workers leaves those descendants reparented to init
/// on cancellation. The child leads its own group (see `process_group(0)`
/// below), so signalling the negative pgid reaps the whole subtree. The guard
/// is disarmed once the child has exited normally, so a recycled group id is
/// never signalled.
#[cfg(unix)]
struct ProcessGroupKiller {
    pgid: i32,
    armed: bool,
}

#[cfg(unix)]
impl ProcessGroupKiller {
    fn new(pid: u32) -> Self {
        Self {
            pgid: pid as i32,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(unix)]
impl Drop for ProcessGroupKiller {
    fn drop(&mut self) {
        if self.armed {
            // SAFETY: `kill(2)` with a negative pgid targets every process in
            // that group. The group was created by `process_group(0)`, so its
            // id equals the child leader's pid; the call has no other effect.
            unsafe {
                libc::kill(-self.pgid, libc::SIGKILL);
            }
        }
    }
}

pub struct LocalExecutor;

impl LocalExecutor {
    pub fn new() -> Self {
        LocalExecutor
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Characters only a shell can interpret. Passing one to `exec` would hand it to
/// the program as a literal argument instead of acting on it.
const SHELL_META: &[char] = &[
    '|', '&', ';', '<', '>', '(', ')', '$', '`', '\\', '"', '\'', '\n', '*', '?', '[', ']', '#',
    '~', '=', '%', '{', '}', '!',
];

/// Words the shell handles itself. Either there is no binary to exec, or execing
/// one would silently do nothing: a `cd` in a child process changes nothing the
/// next command could observe. `echo` is here despite having a binary, because
/// the builtin and `/bin/echo` disagree on flags such as `-e`, and that is not
/// worth a behaviour change for one saved fork.
const SHELL_BUILTINS: &[&str] = &[
    "cd", "export", "source", ".", ":", "eval", "exec", "set", "unset", "alias", "unalias",
    "readonly", "shift", "trap", "umask", "wait", "times", "ulimit", "hash", "type", "command",
    "local", "return", "break", "continue", "echo",
];

/// The argv to exec directly, when the beam needs no shell at all.
///
/// `None` means "spawn a shell", and is the safe answer: more than one command
/// (they share a single shell, so a `cd` in the first is visible to the second),
/// any shell metacharacter, or a builtin. Skipping the shell must change how a
/// beam is started, never what it does.
pub fn direct_argv(commands: &[String]) -> Option<Vec<String>> {
    let [only] = commands else { return None };
    let trimmed = only.trim();
    if trimmed.is_empty() || trimmed.contains(SHELL_META) {
        return None;
    }
    let argv: Vec<String> = trimmed.split_whitespace().map(str::to_string).collect();
    if SHELL_BUILTINS.contains(&argv.first()?.as_str()) {
        return None;
    }
    Some(argv)
}

/// Resolves `program` to a path, searching `path_var` when it is a bare name.
///
/// Two reasons, and the second is the surprising one:
///
/// 1. Correctness. A beam's environment is the Beamfile's, not Aurora's (see
///    `env_clear` below), so the `PATH` it declares is the one that must resolve
///    its command. Handing a bare name to the spawn call would search Aurora's
///    own `PATH` and could run a binary the Beamfile never asked for.
/// 2. Speed. Rust's standard library can only use `posix_spawn` when the program
///    is already a path; a bare name makes it fall back to `fork` + `exec`, and
///    forking Aurora (24 MB, it links wasmtime) copies page tables that
///    `posix_spawn` never touches. Measured, it doubles the cost of every spawn.
pub fn resolve_program(program: &str, path_var: Option<&String>) -> Option<PathBuf> {
    if program.contains('/') {
        return Some(PathBuf::from(program));
    }
    path_var?
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(program))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[async_trait]
impl Executor for LocalExecutor {
    fn name(&self) -> &str {
        "local"
    }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let path_var = input.env.get("PATH");

        let mut command = match direct_argv(&input.commands) {
            Some(argv) => {
                let program = match (resolve_program(&argv[0], path_var), path_var) {
                    (Some(path), _) => path,
                    // A declared PATH is authoritative: a command missing from it
                    // is missing. Letting the OS search Aurora's own PATH instead
                    // would run something the Beamfile never asked for.
                    (None, Some(_)) => bail!("command not found in PATH: {}", argv[0]),
                    // No PATH declared at all: leave the resolution to the OS, as
                    // before. Slower, but it cannot fail a beam that used to run.
                    (None, None) => PathBuf::from(&argv[0]),
                };
                let mut command = Command::new(program);
                command.args(&argv[1..]);
                command
            }
            None => {
                let script = format!("set -e\n{}", input.commands.join("\n"));
                // The shell is Aurora's own tool, not the beam's command: fall
                // back to the system one rather than failing a beam whose
                // declared PATH happens not to contain a shell.
                let shell =
                    resolve_program("sh", path_var).unwrap_or_else(|| PathBuf::from("/bin/sh"));
                let mut command = Command::new(shell);
                command.arg("-c").arg(script);
                command
            }
        };

        // env_clear(): the child process must NOT inherit Aurora's ambient
        // environment (CI secrets, keys, etc.). The env provided in
        // `input.env` is authoritative (see aurora-core/src/env.rs).
        // stdin is detached, never inherited: a beam is a batch task whose
        // output we capture, not an interactive session. A command that probes
        // for a terminal (docker, git, npm) would otherwise switch to
        // interactive mode and contend with the execution TUI for the very
        // terminal it holds in raw mode.
        command
            .current_dir(&input.working_dir)
            .env_clear()
            .envs(&input.env)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Run the child as the leader of its own process group so cancellation
        // can reap its whole subtree, not just the direct `sh` (see
        // `ProcessGroupKiller`).
        #[cfg(unix)]
        command.process_group(0);

        let child = command.spawn()?;

        #[cfg(unix)]
        let mut group_killer = child.id().map(ProcessGroupKiller::new);

        let output = pump_child(child, input.output_tx).await;

        // The child exited on its own: disarm the guard so its process group is
        // not signalled after the fact (the id may have been recycled).
        #[cfg(unix)]
        if let Some(killer) = group_killer.as_mut() {
            killer.disarm();
        }

        output
    }
}
