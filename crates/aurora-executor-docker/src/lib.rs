use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use tokio::process::Command;

pub struct DockerExecutor;

impl DockerExecutor {
    pub fn new() -> Self { DockerExecutor }
}

impl Default for DockerExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Executor for DockerExecutor {
    fn name(&self) -> &str { "docker" }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let image = input.config["image"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Docker executor requires 'image' config"))?
            .to_string();

        let working_dir_str = input.working_dir.to_string_lossy().to_string();
        let volumes = input.config["volumes"]
            .as_array()
            .map(|v| v.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![format!("{}:/app:rw", working_dir_str)]);

        let script = format!("set -e\n{}", input.commands.join("\n"));

        let mut cmd = Command::new("docker");
        cmd.arg("run").arg("--rm");
        cmd.arg("-w").arg("/app");

        for vol in &volumes {
            cmd.arg("-v").arg(vol);
        }

        for (k, v) in &input.env {
            cmd.arg("-e").arg(format!("{}={}", k, v));
        }

        cmd.arg(&image)
           .arg("sh")
           .arg("-c")
           .arg(&script)
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());

        let output = cmd.output().await?;

        Ok(ExecutionOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
