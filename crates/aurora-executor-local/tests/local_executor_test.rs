use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;

/// The executor no longer inherits the ambient environment: at least PATH
/// must therefore be provided so that `sh` and binaries can be resolved.
fn base_env() -> HashMap<String, String> {
    HashMap::from([(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )])
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
        commands: vec!["echo line1".to_string(), "echo line2".to_string()],
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

// A non-UTF-8 byte in the output must not terminate the stream: output
// emitted after the invalid byte must still be captured.
#[tokio::test]
async fn test_invalid_utf8_does_not_truncate_output() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["printf 'before\\n'; printf '\\375\\376\\n'; printf 'after\\n'".to_string()],
        env: base_env(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("before"), "missing 'before': {stdout:?}");
    assert!(
        stdout.contains("after"),
        "output truncated after invalid byte: {stdout:?}"
    );
}

// A non-UTF-8 byte followed by a large amount of output must not leave the
// pipe undrained: the child must not be killed by SIGPIPE, and a command
// that would exit 0 must still report exit 0.
#[tokio::test]
async fn test_invalid_utf8_does_not_kill_child() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["printf '\\375\\n'; seq 1 100000; echo all-done".to_string()],
        env: base_env(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0, "child killed by SIGPIPE");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("all-done"),
        "output truncated before completion"
    );
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
