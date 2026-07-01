use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::time::Duration;

// Dropping the execution future must kill the child `sh`. We launch a
// `sleep 2 && touch marker`, drop the future after 200 ms, then verify
// that no marker appears: proof that the child was indeed killed.
#[tokio::test]
async fn test_kill_on_drop_terminates_child() {
    let dir = std::env::temp_dir().join(format!("aurora_kill_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("marker");
    let _ = std::fs::remove_file(&marker);

    // Keep PATH: env_clear() empties the child's environment, otherwise
    // `sleep`/`touch` might not be resolved.
    let mut env = HashMap::new();
    if let Ok(path) = std::env::var("PATH") {
        env.insert("PATH".to_string(), path);
    }

    let exec = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec![format!("sleep 2 && touch {}", marker.display())],
        env,
        working_dir: dir.clone(),
        config: serde_json::json!({}),
        output_tx: None,
    };

    let fut = exec.execute(input);
    tokio::select! {
        _ = fut => panic!("the future should not have completed within 200 ms"),
        _ = tokio::time::sleep(Duration::from_millis(200)) => {}
    }
    // `fut` is dropped here.

    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !marker.exists(),
        "the child should have been killed (kill_on_drop)"
    );
}
