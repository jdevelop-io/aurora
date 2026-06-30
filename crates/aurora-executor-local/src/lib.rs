use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use tokio::io::{AsyncBufReadExt, BufReader};
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

        // env_clear() : le processus enfant ne doit PAS hériter de
        // l'environnement ambiant d'Aurora (secrets CI, clés, etc.). L'env
        // fourni dans `input.env` fait autorité (voir aurora-core/src/env.rs).
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .current_dir(&input.working_dir)
            .env_clear()
            .envs(&input.env)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let tx_out = input.output_tx.clone();
        let tx_err = input.output_tx.clone();

        let (stdout_lines, stderr_lines, status) = tokio::join!(
            async move {
                let mut reader = BufReader::new(stdout).lines();
                let mut lines = vec![];
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Some(ref tx) = tx_out {
                        let _ = tx.send((line.clone(), false)).await;
                    }
                    lines.push(line);
                }
                lines
            },
            async move {
                let mut reader = BufReader::new(stderr).lines();
                let mut lines = vec![];
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Some(ref tx) = tx_err {
                        let _ = tx.send((line.clone(), true)).await;
                    }
                    lines.push(line);
                }
                lines
            },
            child.wait(),
        );

        let exit_code = status?.code().unwrap_or(-1);

        Ok(ExecutionOutput {
            exit_code,
            stdout: stdout_lines.join("\n").into_bytes(),
            stderr: stderr_lines.join("\n").into_bytes(),
        })
    }
}
