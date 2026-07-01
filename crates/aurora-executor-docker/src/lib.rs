use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Rejects volume mounts that would allow a container escape
/// (Docker socket -> daemon control, host root or system paths).
///
/// Only applies to volumes provided explicitly via the config; the
/// default mount (working_dir) is not affected.
fn validate_volume(spec: &str) -> Result<()> {
    // Format: hostPath[:containerPath[:mode]]
    let host_path = spec.split(':').next().unwrap_or("");
    let normalized = host_path.trim_end_matches('/');

    if normalized.is_empty() {
        anyhow::bail!("forbidden volume (host root \"/\"): {spec}");
    }

    const FORBIDDEN: &[&str] = &[
        "/var/run/docker.sock",
        "/run/docker.sock",
        "/proc",
        "/sys",
        "/dev",
        "/etc",
        "/boot",
        "/root",
        "/var/run",
        "/run",
        "/var/lib/docker",
    ];
    for p in FORBIDDEN {
        if normalized == *p || normalized.starts_with(&format!("{p}/")) {
            anyhow::bail!("forbidden volume (system path {p}): {spec}");
        }
    }
    Ok(())
}

pub struct DockerExecutor;

impl DockerExecutor {
    pub fn new() -> Self {
        DockerExecutor
    }
}

impl Default for DockerExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Executor for DockerExecutor {
    fn name(&self) -> &str {
        "docker"
    }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let image = input.config["image"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Docker executor requires 'image' config"))?
            .to_string();

        let working_dir_str = input.working_dir.to_string_lossy().to_string();
        let volumes = match input.config["volumes"].as_array() {
            Some(arr) => {
                let vols: Vec<String> = arr
                    .iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .collect();
                // Volumes provided via the config are validated: we fail
                // rather than silently mounting a dangerous path.
                for v in &vols {
                    validate_volume(v)?;
                }
                vols
            }
            None => vec![format!("{}:/app:rw", working_dir_str)],
        };

        let script = format!("set -e\n{}", input.commands.join("\n"));

        let mut cmd = Command::new("docker");
        cmd.kill_on_drop(true);
        cmd.arg("run").arg("--rm");
        // Prevents privilege escalation via setuid binaries in the image.
        cmd.arg("--security-opt").arg("no-new-privileges");
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

        let mut child = cmd.spawn()?;
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
