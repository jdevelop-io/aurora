use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert("local".into(), Arc::new(LocalExecutor::new()));
    m
}

fn make_beam(name: &str, deps: Vec<&str>, commands: Vec<&str>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: vec![],
        outputs: vec![],
        variables: vec![],
        args: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: if commands.is_empty() {
            None
        } else {
            Some(Run {
                commands: commands.iter().map(|s| s.to_string()).collect(),
                executor: None,
            })
        },
        allow_failure: false,
    }
}

// `slow` runs for a long time and has a dependent `after`. `independent` is
// short and unrelated. We cancel `slow` while it is running: `slow` ->
// Cancelled, `after` -> Cancelled, `independent` -> Success, and the overall
// run fails.
#[tokio::test]
async fn test_cancel_running_beam_cancels_it_and_dependents() {
    let beams = vec![
        make_beam("slow", vec![], vec!["sleep 30"]),
        make_beam("after", vec!["slow"], vec!["echo after"]),
        make_beam("independent", vec![], vec!["echo indep"]),
        make_beam("root", vec!["after", "independent"], vec!["echo root"]),
    ];

    let (tx, mut rx) = mpsc::channel(256);
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();

    let handle = tokio::spawn(async move {
        Scheduler::new(
            beams,
            local_executors(),
            tx,
            None,
            std::path::PathBuf::from("/tmp"),
            HashMap::new(),
        )
        .run_cancellable("root", &[], cancel_rx)
        .await
        .unwrap()
    });

    // Wait for `slow` to start, then cancel it.
    let mut slow_started = false;
    let mut events = vec![];
    while let Some(evt) = rx.recv().await {
        if let SchedulerEvent::BeamStarted { name } = &evt {
            if name == "slow" && !slow_started {
                slow_started = true;
                cancel_tx.send("slow".to_string()).unwrap();
            }
        }
        let done = matches!(evt, SchedulerEvent::AllDone { .. });
        events.push(evt);
        if done {
            break;
        }
    }

    let overall = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the scheduler did not finish after cancellation")
        .unwrap();

    let status_of = |beam: &str| -> Option<BeamStatus> {
        events.iter().rev().find_map(|e| match e {
            SchedulerEvent::BeamCompleted { name, status } if name == beam => Some(status.clone()),
            _ => None,
        })
    };

    assert!(slow_started, "slow should have started");
    assert!(
        matches!(status_of("slow"), Some(BeamStatus::Cancelled)),
        "slow must be cancelled"
    );
    assert!(
        matches!(status_of("after"), Some(BeamStatus::Cancelled)),
        "after (dependent) must be cancelled"
    );
    assert!(
        matches!(status_of("independent"), Some(BeamStatus::Success { .. })),
        "independent must succeed"
    );
    assert!(!overall, "the overall run must fail after a cancellation");

    let done_ok = events
        .iter()
        .any(|e| matches!(e, SchedulerEvent::AllDone { success: false }));
    assert!(done_ok, "AllDone must report success=false");
}

// A beam blocked evaluating a long-running `skip_if` gate must be
// cancellable: the gate command is raced against cancellation, so the run
// tears down promptly instead of hanging until the gate exits on its own.
#[tokio::test]
async fn test_cancel_beam_blocked_in_skip_if_gate() {
    let mut gated = make_beam("gated", vec![], vec!["echo ran"]);
    gated.skip_if = Some("sleep 30".to_string());

    let (tx, mut rx) = mpsc::channel(256);
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();

    let handle = tokio::spawn(async move {
        Scheduler::new(
            vec![gated],
            local_executors(),
            tx,
            None,
            std::path::PathBuf::from("/tmp"),
            HashMap::new(),
        )
        .run_cancellable("gated", &[], cancel_rx)
        .await
        .unwrap()
    });

    // `BeamStarted` is emitted before the gate is evaluated, so waiting for it
    // guarantees the task is parked inside the gate when we cancel.
    let mut started = false;
    let mut events = vec![];
    while let Some(evt) = rx.recv().await {
        if let SchedulerEvent::BeamStarted { name } = &evt {
            if name == "gated" && !started {
                started = true;
                cancel_tx.send("gated".to_string()).unwrap();
            }
        }
        let done = matches!(evt, SchedulerEvent::AllDone { .. });
        events.push(evt);
        if done {
            break;
        }
    }

    // Without racing the gate against cancellation, the run would sit on
    // `sleep 30` and this timeout (5s) would elapse first.
    let overall = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the run must stop promptly when a gated beam is cancelled")
        .unwrap();

    let status_of = |beam: &str| -> Option<BeamStatus> {
        events.iter().rev().find_map(|e| match e {
            SchedulerEvent::BeamCompleted { name, status } if name == beam => Some(status.clone()),
            _ => None,
        })
    };

    assert!(started, "gated should have started");
    assert!(
        matches!(status_of("gated"), Some(BeamStatus::Cancelled)),
        "a beam cancelled while gating must display as Cancelled"
    );
    assert!(!overall, "the run must fail after a cancellation");
}

// Cancelling an `allow_failure` beam behaves like a tolerated failure: its
// dependent still runs (unblocked) and the overall run stays green. The
// cancelled beam still displays as `Cancelled`.
#[tokio::test]
async fn test_cancel_allow_failure_beam_does_not_cancel_dependents() {
    let mut slow = make_beam("slow", vec![], vec!["sleep 30"]);
    slow.allow_failure = true;
    let after = make_beam("after", vec!["slow"], vec!["echo after"]);

    let (tx, mut rx) = mpsc::channel(256);
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();

    let handle = tokio::spawn(async move {
        Scheduler::new(
            vec![slow, after],
            local_executors(),
            tx,
            None,
            std::path::PathBuf::from("/tmp"),
            HashMap::new(),
        )
        .run_cancellable("after", &[], cancel_rx)
        .await
        .unwrap()
    });

    let mut slow_started = false;
    let mut events = vec![];
    while let Some(evt) = rx.recv().await {
        if let SchedulerEvent::BeamStarted { name } = &evt {
            if name == "slow" && !slow_started {
                slow_started = true;
                cancel_tx.send("slow".to_string()).unwrap();
            }
        }
        let done = matches!(evt, SchedulerEvent::AllDone { .. });
        events.push(evt);
        if done {
            break;
        }
    }

    let overall = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the scheduler did not finish after cancellation")
        .unwrap();

    let status_of = |beam: &str| -> Option<BeamStatus> {
        events.iter().rev().find_map(|e| match e {
            SchedulerEvent::BeamCompleted { name, status } if name == beam => Some(status.clone()),
            _ => None,
        })
    };

    assert!(slow_started, "slow should have started");
    assert!(
        matches!(status_of("slow"), Some(BeamStatus::Cancelled)),
        "cancelled slow must display as Cancelled"
    );
    assert!(
        matches!(status_of("after"), Some(BeamStatus::Success { .. })),
        "after must run despite slow being cancelled (allow_failure)"
    );
    assert!(
        overall,
        "cancelling an allow_failure beam must not fail the run"
    );
}
