use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionInput {
    pub commands: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
    /// Executor-specific configuration (e.g. {"image": "ubuntu:22.04"} for docker)
    pub config: serde_json::Value,
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
