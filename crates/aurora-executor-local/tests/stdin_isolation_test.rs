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

/// A beam is a batch task whose output Aurora captures, never an interactive
/// session, so its stdin must be detached from Aurora's own.
///
/// Letting the child inherit it is not merely untidy: a command that probes
/// for a terminal (docker, git, npm, Symfony Console) then switches to
/// interactive mode. Under the execution TUI, which holds that same terminal
/// in raw mode, `docker run --interactive --tty` ends up spinning after its
/// container has exited, relaying no output and never terminating, so the beam
/// stays Running with an empty log pane forever.
///
/// Resolving fd 0 is Linux-specific; the guarantee itself (`Stdio::null`)
/// is not.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_child_stdin_is_detached_from_aurora() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["readlink /proc/self/fd/0".to_string()],
        env: base_env(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "/dev/null",
        "the beam's stdin must be /dev/null, not whatever Aurora was given"
    );
}

/// A command reading stdin must see end-of-file at once rather than block on
/// input nobody will ever type.
#[tokio::test]
async fn test_child_reading_stdin_gets_eof_immediately() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["cat; echo EOF_REACHED".to_string()],
        env: base_env(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
        output_tx: None,
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "EOF_REACHED",
        "the beam must read nothing from stdin"
    );
}
