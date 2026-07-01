use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{pump_child, ExecutionInput, ExecutionOutput, Executor};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::process::Command;

/// Monotonic counter making each container name unique within this process.
static CONTAINER_SEQ: AtomicU64 = AtomicU64::new(0);

/// Builds a container name that is unique within this process (the counter)
/// and across processes (the pid), so a beam's container can be addressed by
/// name for removal on cancellation.
fn next_container_name() -> String {
    let seq = CONTAINER_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("aurora-{}-{}", std::process::id(), seq)
}

/// Builds the full `docker run` argument vector. Pure (no side effects) so the
/// argument construction, including the `--name` that lets cancellation reap
/// the container, can be tested without a Docker daemon.
pub fn build_run_args(
    name: &str,
    image: &str,
    volumes: &[String],
    env: &HashMap<String, String>,
    script: &str,
) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        // Name the container so it can be removed by name if the `docker run`
        // client is killed on cancellation (see `ContainerCleanup`).
        "--name".to_string(),
        name.to_string(),
        // Prevents privilege escalation via setuid binaries in the image.
        "--security-opt".to_string(),
        "no-new-privileges".to_string(),
        "-w".to_string(),
        "/app".to_string(),
    ];

    for vol in volumes {
        args.push("-v".to_string());
        args.push(vol.clone());
    }
    for (k, v) in env {
        args.push("-e".to_string());
        args.push(format!("{k}={v}"));
    }

    // `--` terminates `docker run` options: everything after it is the image
    // and its command, so a crafted image can never inject a flag.
    args.push("--".to_string());
    args.push(image.to_string());
    args.push("sh".to_string());
    args.push("-c".to_string());
    args.push(script.to_string());
    args
}

/// Removes the container by name with `docker rm -f` when dropped while still
/// armed. `kill_on_drop` SIGKILLs the `docker run` client, but the container
/// is a child of the daemon: it keeps running and `--rm` never fires. Removing
/// it by name talks to the daemon directly, so it works even though the client
/// is already dead. Disarmed on normal completion, where `--rm` suffices.
struct ContainerCleanup {
    name: String,
    armed: bool,
}

impl ContainerCleanup {
    fn new(name: String) -> Self {
        Self { name, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ContainerCleanup {
    fn drop(&mut self) {
        if self.armed {
            // Synchronous on the cancellation path (Drop cannot be async); the
            // call is short and its failure must never surface.
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", &self.name])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

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

    // Docker resolves symlinks and `..` host-side at mount time, so a symlink
    // such as `/tmp/x -> /var/run/docker.sock` (or a `..` escape) would bypass
    // a purely textual blocklist and hand the daemon socket to the container.
    // We therefore compare the resolved (canonical) host path against the
    // resolved forbidden paths, and also keep the raw textual comparison for
    // paths that do not exist yet (a non-existent path cannot be a symlink).
    let canonicalize = |p: &str| {
        std::fs::canonicalize(p)
            .ok()
            .map(|c| c.to_string_lossy().trim_end_matches('/').to_string())
    };

    let candidates: Vec<String> = std::iter::once(normalized.to_string())
        .chain(canonicalize(host_path))
        .collect();

    for p in FORBIDDEN {
        // Compare against the raw forbidden path and its canonical form, so
        // platform aliases (macOS `/etc` -> `/private/etc`) still match.
        let targets: Vec<String> = std::iter::once(p.to_string())
            .chain(canonicalize(p))
            .collect();
        for candidate in &candidates {
            for target in &targets {
                if candidate == target || candidate.starts_with(&format!("{target}/")) {
                    anyhow::bail!("forbidden volume (system path {p}): {spec}");
                }
            }
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

        // An image starting with '-' would be parsed by `docker run` as an
        // option (e.g. --privileged, --network=host), bypassing the volume
        // validation entirely. Reject it before any container launch.
        if image.is_empty() || image.starts_with('-') {
            anyhow::bail!("invalid Docker image name: {image:?}");
        }

        let working_dir_str = input.working_dir.to_string_lossy().to_string();
        // `volumes` is a comma-separated list of `-v` specs. The executor
        // config only carries strings (see aurora-core `build_executor_config`
        // and the Beamfile grammar, where executor fields are strings), so a
        // JSON array would never reach here through the real pipeline. Absent
        // or empty falls back to the default working-directory mount.
        let requested: Vec<String> = input
            .config
            .get("volumes")
            .and_then(|v| v.as_str())
            .map(|spec| {
                spec.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let volumes = if requested.is_empty() {
            vec![format!("{}:/app:rw", working_dir_str)]
        } else {
            // Volumes provided via the config are validated: we fail rather
            // than silently mounting a dangerous path.
            for v in &requested {
                validate_volume(v)?;
            }
            requested
        };

        let script = format!("set -e\n{}", input.commands.join("\n"));

        let name = next_container_name();
        let args = build_run_args(&name, &image, &volumes, &input.env, &script);

        let mut cmd = Command::new("docker");
        cmd.kill_on_drop(true)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let child = cmd.spawn()?;

        // Armed while the container runs: on cancellation the execution future
        // is dropped and the guard removes the container by name.
        let mut cleanup = ContainerCleanup::new(name);
        let output = pump_child(child, input.output_tx).await;

        // The container exited on its own; `--rm` removed it, so nothing left
        // to force-remove.
        cleanup.disarm();
        output
    }
}
