use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Child;
use tokio::sync::mpsc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionInput {
    pub commands: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
    /// Executor-specific configuration (e.g. {"image": "ubuntu:22.04"} for docker)
    pub config: serde_json::Value,
    /// Optional channel to stream output lines in real time.
    /// `(line, is_stderr)`
    #[serde(skip)]
    pub output_tx: Option<mpsc::Sender<(String, bool)>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl ExecutionOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Trait implemented by all executors: local shell, docker, WASM plugins.
#[async_trait]
pub trait Executor: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput>;
}

/// Reads a stream line by line, forwarding each line to `output_tx` (when set)
/// and collecting them for the final [`ExecutionOutput`].
async fn collect_stream<R>(
    reader: R,
    is_stderr: bool,
    output_tx: Option<mpsc::Sender<(String, bool)>>,
) -> Vec<String>
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut collected = vec![];
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(ref tx) = output_tx {
            let _ = tx.send((line.clone(), is_stderr)).await;
        }
        collected.push(line);
    }
    collected
}

/// Streams a spawned child's stdout and stderr (forwarding lines to
/// `output_tx` in real time), waits for it to exit, and builds the
/// [`ExecutionOutput`]. Shared by the process-based executors (local, docker),
/// which only differ in how they build the [`Child`].
///
/// The child must be spawned with both stdout and stderr piped.
pub async fn pump_child(
    mut child: Child,
    output_tx: Option<mpsc::Sender<(String, bool)>>,
) -> Result<ExecutionOutput> {
    let stdout = child
        .stdout
        .take()
        .expect("child stdout must be piped for pump_child");
    let stderr = child
        .stderr
        .take()
        .expect("child stderr must be piped for pump_child");

    let (stdout_lines, stderr_lines, status) = tokio::join!(
        collect_stream(stdout, false, output_tx.clone()),
        collect_stream(stderr, true, output_tx),
        child.wait(),
    );

    Ok(ExecutionOutput {
        exit_code: status?.code().unwrap_or(-1),
        stdout: stdout_lines.join("\n").into_bytes(),
        stderr: stderr_lines.join("\n").into_bytes(),
    })
}
