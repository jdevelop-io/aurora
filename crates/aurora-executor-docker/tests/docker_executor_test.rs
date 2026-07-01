// These tests require Docker. Marked #[ignore] by default.
use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_docker::DockerExecutor;
use std::collections::HashMap;

// Does NOT require Docker: validates that dangerous volumes are rejected
// before any container launch (defense against sandbox escape).
#[tokio::test]
async fn test_dangerous_volume_is_rejected() {
    let executor = DockerExecutor::new();
    for vol in [
        "/var/run/docker.sock:/var/run/docker.sock",
        "/:/host:rw",
        "/etc:/etc",
        "/proc:/proc",
    ] {
        let input = ExecutionInput {
            commands: vec!["echo nope".to_string()],
            env: HashMap::new(),
            working_dir: std::env::current_dir().unwrap(),
            // `volumes` is a comma-separated string, matching what the
            // executor config actually carries through the real pipeline.
            config: serde_json::json!({ "image": "alpine:3.19", "volumes": vol }),
            output_tx: None,
        };
        let result = executor.execute(input).await;
        assert!(result.is_err(), "dangerous volume accepted: {vol}");
    }
}

// Does NOT require Docker: a dangerous volume hidden among several
// comma-separated specs must still be rejected.
#[tokio::test]
async fn test_dangerous_volume_in_list_is_rejected() {
    let executor = DockerExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo nope".to_string()],
        env: HashMap::new(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({
            "image": "alpine:3.19",
            "volumes": "/tmp/ok:/ok:ro,/etc:/etc:ro",
        }),
        output_tx: None,
    };
    let result = executor.execute(input).await;
    assert!(result.is_err(), "dangerous volume in a list accepted");
}

// Does NOT require Docker: a symlink pointing at a forbidden path must not
// slip past the textual blocklist. Docker resolves the symlink host-side at
// mount time, so a purely textual check would let it through.
#[cfg(unix)]
#[tokio::test]
async fn test_symlink_to_forbidden_path_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let link = tmp.path().join("sneaky");
    std::os::unix::fs::symlink("/etc", &link).unwrap();
    let spec = format!("{}:/host:rw", link.display());

    let executor = DockerExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo nope".to_string()],
        env: HashMap::new(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({ "image": "alpine:3.19", "volumes": spec }),
        output_tx: None,
    };
    let result = executor.execute(input).await;
    assert!(result.is_err(), "symlink to /etc accepted");
}

// Does NOT require Docker: an image field starting with '-' would be parsed
// as a `docker run` flag (e.g. --privileged), bypassing volume validation.
#[tokio::test]
async fn test_image_starting_with_dash_is_rejected() {
    let executor = DockerExecutor::new();
    for image in ["--privileged", "-v/:/host", ""] {
        let input = ExecutionInput {
            commands: vec!["echo nope".to_string()],
            env: HashMap::new(),
            working_dir: std::env::current_dir().unwrap(),
            config: serde_json::json!({ "image": image }),
            output_tx: None,
        };
        let result = executor.execute(input).await;
        assert!(result.is_err(), "dangerous image accepted: {image:?}");
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn test_docker_echo() {
    let executor = DockerExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo hello_from_docker".to_string()],
        env: HashMap::new(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({ "image": "alpine:3.19" }),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("hello_from_docker"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn test_docker_env_vars() {
    let executor = DockerExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo $MY_VAR".to_string()],
        env: HashMap::from([("MY_VAR".to_string(), "aurora_docker".to_string())]),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({ "image": "alpine:3.19" }),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("aurora_docker"));
}
