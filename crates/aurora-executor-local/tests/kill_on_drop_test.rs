use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::time::Duration;

// Abandonner le future d'exécution doit tuer le `sh` enfant. On lance un
// `sleep 2 && touch marker`, on drop le future après 200 ms, puis on vérifie
// qu'aucun marker n'apparaît : preuve que l'enfant a bien été tué.
#[tokio::test]
async fn test_kill_on_drop_terminates_child() {
    let dir = std::env::temp_dir().join(format!("aurora_kill_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("marker");
    let _ = std::fs::remove_file(&marker);

    // Conserver PATH : env_clear() vide l'environnement de l'enfant, sinon
    // `sleep`/`touch` peuvent ne pas être résolus.
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
        _ = fut => panic!("le future ne devait pas se terminer en 200 ms"),
        _ = tokio::time::sleep(Duration::from_millis(200)) => {}
    }
    // `fut` est drop ici.

    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !marker.exists(),
        "l'enfant aurait dû être tué (kill_on_drop)"
    );
}
