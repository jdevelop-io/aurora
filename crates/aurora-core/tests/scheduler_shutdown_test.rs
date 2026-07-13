//! A shutdown request (Ctrl-C, SIGTERM) must stop the whole run: every running
//! beam is cancelled, no further beam is spawned, and the run reports failure.

use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

fn beam(name: &str, depends_on: Vec<String>, commands: Vec<String>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on,
        inputs: vec![],
        outputs: vec![],
        variables: vec![],
        args: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands,
            executor: None,
        }),
        allow_failure: false,
    }
}

fn executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut map: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    map.insert("local".into(), Arc::new(LocalExecutor::new()));
    map
}

/// `slow` sleeps well past the shutdown; `after` must never get to run.
fn slow_then_after() -> Vec<Beam> {
    vec![
        beam("slow", vec![], vec!["sleep 30".to_string()]),
        beam(
            "after",
            vec!["slow".to_string()],
            vec!["echo after".to_string()],
        ),
    ]
}

#[tokio::test]
async fn shutdown_cancels_the_running_beam_and_fails_the_run() {
    let (tx, mut rx) = mpsc::channel(64);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let scheduler = Scheduler::new(
        slow_then_after(),
        executors(),
        tx,
        None,
        std::env::temp_dir(),
        HashMap::new(),
    )
    .without_cache()
    .with_shutdown(shutdown_rx);

    let handle = tokio::spawn(async move { scheduler.run("after", &[]).await });

    tokio::time::sleep(Duration::from_millis(300)).await;
    shutdown_tx.send(()).unwrap();

    // Without a shutdown path the run would sit on `sleep 30`.
    let success = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the run must stop promptly on shutdown")
        .unwrap()
        .unwrap();

    assert!(!success, "a shut-down run must not report success");

    let mut slow_cancelled = false;
    while let Ok(event) = rx.try_recv() {
        if let SchedulerEvent::BeamCompleted { name, status } = event {
            if name == "slow" {
                assert!(
                    matches!(status, BeamStatus::Cancelled),
                    "the running beam must be reported Cancelled, got {status:?}"
                );
                slow_cancelled = true;
            }
        }
    }
    assert!(
        slow_cancelled,
        "the running beam must emit a terminal event"
    );
}

#[tokio::test]
async fn shutdown_spawns_no_further_beam() {
    let (tx, mut rx) = mpsc::channel(64);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let scheduler = Scheduler::new(
        slow_then_after(),
        executors(),
        tx,
        None,
        std::env::temp_dir(),
        HashMap::new(),
    )
    .without_cache()
    .with_shutdown(shutdown_rx);

    let handle = tokio::spawn(async move { scheduler.run("after", &[]).await });

    tokio::time::sleep(Duration::from_millis(300)).await;
    shutdown_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the run must stop promptly on shutdown")
        .unwrap()
        .unwrap();

    while let Ok(event) = rx.try_recv() {
        if let SchedulerEvent::BeamStarted { name, .. } = event {
            assert_ne!(
                name, "after",
                "no beam may start once shutdown has been requested"
            );
        }
    }
}
