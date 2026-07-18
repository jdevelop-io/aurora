//! A shutdown request (Ctrl-C, SIGTERM) must stop the whole run: every running
//! beam is cancelled, no further beam is spawned, and the run reports failure.

use aurora_core::ast::{Beam, Dependency, Run};
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
        depends_on: depends_on.into_iter().map(Dependency::named).collect(),
        run: Some(Run {
            commands,
            executor: None,
        }),
        ..Beam::default()
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

/// Arming a shutdown then dropping the sender without sending means the run
/// can never be shut down. The scheduler must treat it like the absent case
/// and run to completion, not panic by re-polling a completed receiver.
#[tokio::test]
async fn dropped_shutdown_sender_does_not_panic() {
    let (tx, _rx) = mpsc::channel(256);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // Two beams (sequential) so the scheduler's loop iterates more than once,
    // which is what re-polls the shutdown receiver.
    let beams = vec![
        beam("first", vec![], vec!["echo first".to_string()]),
        beam(
            "second",
            vec!["first".to_string()],
            vec!["echo second".to_string()],
        ),
    ];

    let scheduler = Scheduler::new(
        beams,
        executors(),
        tx,
        None,
        std::env::temp_dir(),
        HashMap::new(),
    )
    .without_cache()
    .with_shutdown(shutdown_rx);

    // Drop the sender without sending: the receiver resolves with an error.
    drop(shutdown_tx);

    let handle = tokio::spawn(async move { scheduler.run("second", &[]).await });

    let result = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the run must finish");

    let success = result
        .expect("the scheduler must not panic when the shutdown sender is dropped")
        .unwrap();
    assert!(success, "a run with no shutdown must succeed");
}

/// An `allow_failure` beam running at shutdown is cancelled and counts as Ok
/// for scheduling, but the torn-down run must still report failure: it did
/// not complete, and its downstream beam never ran.
#[tokio::test]
async fn shutdown_fails_the_run_even_with_an_allow_failure_beam() {
    let (tx, _rx) = mpsc::channel(64);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let mut slow = beam("slow", vec![], vec!["sleep 30".to_string()]);
    slow.allow_failure = true;
    let after = beam(
        "after",
        vec!["slow".to_string()],
        vec!["echo after".to_string()],
    );

    let scheduler = Scheduler::new(
        vec![slow, after],
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

    let success = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the run must stop promptly on shutdown")
        .unwrap()
        .unwrap();

    assert!(
        !success,
        "a shut-down run must not report success even when the running beam tolerates failure"
    );
}
