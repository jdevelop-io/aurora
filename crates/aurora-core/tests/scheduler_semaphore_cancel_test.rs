use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert("local".into(), Arc::new(LocalExecutor::new()));
    m
}

fn slow_beam(name: &str) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        variables: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["sleep 30".to_string()],
            executor: None,
        }),
        allow_failure: false,
    }
}

fn aggregate(name: &str, deps: &[&str]) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: vec![],
        outputs: vec![],
        variables: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: None,
        allow_failure: false,
    }
}

/// With a single parallelism slot and two independent slow beams, one runs and
/// the other is queued on the semaphore. Cancelling both must terminate the
/// run promptly with both beams reported Cancelled, proving a beam queued for a
/// slot is not left stuck until it starts.
#[tokio::test]
async fn cancel_reaches_a_beam_queued_on_the_semaphore() {
    let (tx, mut rx) = mpsc::channel(64);
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();

    let scheduler = Scheduler::new(
        vec![
            slow_beam("a"),
            slow_beam("b"),
            aggregate("all", &["a", "b"]),
        ],
        local_executors(),
        tx,
        Some(1), // single slot: one beam necessarily queues behind the other
        std::path::PathBuf::from("/tmp"),
        HashMap::new(),
    );

    let handle =
        tokio::spawn(async move { scheduler.run_cancellable("all", &[], cancel_rx).await });

    // The initial spawn populates the cancel registry synchronously before the
    // event loop, so these buffered cancellations are honored once processed.
    cancel_tx.send("a".to_string()).unwrap();
    cancel_tx.send("b".to_string()).unwrap();

    let mut a_cancelled = false;
    let mut b_cancelled = false;
    while let Some(e) = rx.recv().await {
        if let SchedulerEvent::BeamCompleted {
            name,
            status: BeamStatus::Cancelled,
        } = e
        {
            match name.as_str() {
                "a" => a_cancelled = true,
                "b" => b_cancelled = true,
                _ => {}
            }
        }
    }

    let ok = handle.await.unwrap().unwrap();
    assert!(!ok, "a cancelled run must report failure");
    assert!(a_cancelled, "beam 'a' must be cancelled");
    assert!(b_cancelled, "beam 'b' must be cancelled");
}
