use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;

/// L'exécuteur n'hérite plus de l'environnement ambiant : il faut donc fournir
/// au moins PATH pour que `sh` et les binaires soient résolus.
fn base_env() -> HashMap<String, String> {
    HashMap::from([("PATH".to_string(), std::env::var("PATH").unwrap_or_default())])
}

#[tokio::test]
async fn test_execute_echo() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo hello".to_string()],
        env: base_env(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "hello");
}

#[tokio::test]
async fn test_execute_multi_commands() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec![
            "echo line1".to_string(),
            "echo line2".to_string(),
        ],
        env: base_env(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("line1"));
    assert!(stdout.contains("line2"));
}

#[tokio::test]
async fn test_execute_failing_command() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["false".to_string()],
        env: base_env(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_ne!(output.exit_code, 0);
}

#[tokio::test]
async fn test_env_vars_passed() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo $MY_VAR".to_string()],
        env: {
            let mut e = base_env();
            e.insert("MY_VAR".to_string(), "aurora_test".to_string());
            e
        },
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("aurora_test"));
}
