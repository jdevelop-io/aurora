use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{pump_child, ExecutionInput, ExecutionOutput, Executor};
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

#[async_trait]
impl Executor for LocalExecutor {
    fn name(&self) -> &str {
        "local"
    }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let script = format!("set -e\n{}", input.commands.join("\n"));

        // env_clear(): the child process must NOT inherit Aurora's ambient
        // environment (CI secrets, keys, etc.). The env provided in
        // `input.env` is authoritative (see aurora-core/src/env.rs).
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(&script)
            .current_dir(&input.working_dir)
            .env_clear()
            .envs(&input.env)
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
