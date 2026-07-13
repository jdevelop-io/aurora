//! Skipping the shell must change how a beam is started, never what it does.
//! These drive the executor for real: whatever the spawn strategy, the observable
//! behaviour of a beam has to stay exactly what it was.

use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::path::PathBuf;

fn env_with_path() -> HashMap<String, String> {
    HashMap::from([(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )])
}

async fn run_in(dir: PathBuf, env: HashMap<String, String>, commands: &[&str]) -> ExecutionOutcome {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: commands.iter().map(|c| c.to_string()).collect(),
        env,
        working_dir: dir,
        config: serde_json::json!({}),
        output_tx: None,
    };
    match executor.execute(input).await {
        Ok(output) => ExecutionOutcome {
            exit_code: Some(output.exit_code),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            error: None,
        },
        Err(e) => ExecutionOutcome {
            exit_code: None,
            stdout: String::new(),
            error: Some(e.to_string()),
        },
    }
}

struct ExecutionOutcome {
    exit_code: Option<i32>,
    stdout: String,
    error: Option<String>,
}

#[tokio::test]
async fn a_plain_command_runs_and_reports_its_output() {
    let dir = std::env::current_dir().unwrap();
    let out = run_in(dir, env_with_path(), &["printf hello"]).await;
    assert_eq!(out.exit_code, Some(0), "error: {:?}", out.error);
    assert_eq!(out.stdout, "hello");
}

#[tokio::test]
async fn a_failing_command_reports_its_exit_code() {
    let dir = std::env::current_dir().unwrap();
    let out = run_in(dir, env_with_path(), &["false"]).await;
    assert_eq!(out.exit_code, Some(1), "error: {:?}", out.error);
}

/// The load-bearing one: commands share a single shell, so a `cd` in the first
/// must still be visible to the second.
#[tokio::test]
async fn a_cd_still_carries_to_the_next_command() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();

    let out = run_in(
        dir.path().to_path_buf(),
        env_with_path(),
        &["cd sub", "basename \"$(pwd)\""],
    )
    .await;

    assert_eq!(out.exit_code, Some(0), "error: {:?}", out.error);
    assert_eq!(out.stdout, "sub", "the cd must carry to the next command");
}

#[tokio::test]
async fn shell_features_still_work() {
    let dir = std::env::current_dir().unwrap();
    for (command, expected) in [
        ("printf a && printf b", "ab"),
        ("printf x | tr x y", "y"),
        ("printf \"%s\" quoted", "quoted"),
    ] {
        let out = run_in(dir.clone(), env_with_path(), &[command]).await;
        assert_eq!(
            out.exit_code,
            Some(0),
            "`{command}` failed: {:?}",
            out.error
        );
        assert_eq!(out.stdout, expected, "`{command}`");
    }
}

/// `set -e` aborts a beam on the first failing command, and must keep doing so.
#[tokio::test]
async fn a_failing_command_still_aborts_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let out = run_in(
        dir.path().to_path_buf(),
        env_with_path(),
        &["false", "touch should-not-exist"],
    )
    .await;

    assert_ne!(
        out.exit_code,
        Some(0),
        "a failing command must fail the beam"
    );
    assert!(
        !dir.path().join("should-not-exist").exists(),
        "set -e must still stop the beam at the first failure"
    );
}

/// The declared environment is authoritative: a beam's command must be resolved
/// against the PATH the Beamfile declares, not against Aurora's own.
#[tokio::test]
async fn the_declared_path_resolves_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let tool = dir.path().join("only-here");
    std::fs::write(&tool, "#!/bin/sh\nprintf found\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // A PATH holding only this directory: the command exists nowhere else.
    let env = HashMap::from([("PATH".to_string(), dir.path().display().to_string())]);
    let out = run_in(dir.path().to_path_buf(), env, &["only-here"]).await;

    assert_eq!(out.exit_code, Some(0), "error: {:?}", out.error);
    assert_eq!(out.stdout, "found");
}

/// A command absent from the declared PATH is missing. Falling back to Aurora's
/// own PATH would run a binary the Beamfile never asked for.
#[tokio::test]
async fn a_command_outside_the_declared_path_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let env = HashMap::from([("PATH".to_string(), dir.path().display().to_string())]);

    let out = run_in(dir.path().to_path_buf(), env, &["cargo --version"]).await;

    assert_ne!(
        out.exit_code,
        Some(0),
        "cargo is not in the declared PATH and must not be found through Aurora's"
    );
}

/// A Beamfile that declares no PATH at all used to work, with the OS resolving
/// the command. It must keep working.
#[tokio::test]
async fn a_beam_without_a_declared_path_still_runs() {
    let dir = std::env::current_dir().unwrap();
    let out = run_in(dir, HashMap::new(), &["/bin/echo ok"]).await;
    assert_eq!(out.exit_code, Some(0), "error: {:?}", out.error);
    assert_eq!(out.stdout, "ok");
}
