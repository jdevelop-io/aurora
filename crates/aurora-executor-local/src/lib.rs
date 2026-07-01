use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{pump_child, ExecutionInput, ExecutionOutput, Executor};
use tokio::process::Command;

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
        let child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .current_dir(&input.working_dir)
            .env_clear()
            .envs(&input.env)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        pump_child(child, input.output_tx).await
    }
}
