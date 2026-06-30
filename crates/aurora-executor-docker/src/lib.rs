use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Rejette les montages de volumes qui permettraient une évasion du conteneur
/// (socket Docker → contrôle du démon, racine ou chemins système de l'hôte).
///
/// N'est appliqué qu'aux volumes fournis explicitement via la config ; le
/// montage par défaut (working_dir) n'est pas concerné.
fn validate_volume(spec: &str) -> Result<()> {
    // Format: hostPath[:containerPath[:mode]]
    let host_path = spec.split(':').next().unwrap_or("");
    let normalized = host_path.trim_end_matches('/');

    if normalized.is_empty() {
        anyhow::bail!("volume interdit (racine hôte « / ») : {spec}");
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
            anyhow::bail!("volume interdit (chemin système {p}) : {spec}");
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
                // Les volumes fournis par la config sont validés : on échoue
                // plutôt que de monter silencieusement un chemin dangereux.
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
        // Empêche l'escalade de privilèges via binaires setuid dans l'image.
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
