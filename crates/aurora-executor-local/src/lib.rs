use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use tokio::process::Command;

pub struct LocalExecutor;

impl LocalExecutor {
    pub fn new() -> Self { LocalExecutor }
}

impl Default for LocalExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Executor for LocalExecutor {
    fn name(&self) -> &str { "local" }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let script = format!("set -e\n{}", input.commands.join("\n"));

        let child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .current_dir(&input.working_dir)
            .envs(&input.env)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let output = child.wait_with_output().await?;

        Ok(ExecutionOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
