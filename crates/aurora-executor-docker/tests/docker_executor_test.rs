// Ces tests nécessitent Docker. Marqués #[ignore] par défaut.
use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_docker::DockerExecutor;
use std::collections::HashMap;

// Ne nécessite PAS Docker : valide que les volumes dangereux sont rejetés
// avant tout lancement de conteneur (défense contre l'évasion de sandbox).
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
            config: serde_json::json!({ "image": "alpine:3.19", "volumes": [vol] }),
            output_tx: None,
        };
        let result = executor.execute(input).await;
        assert!(result.is_err(), "volume dangereux accepté : {vol}");
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
