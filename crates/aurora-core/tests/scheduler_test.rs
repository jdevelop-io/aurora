use aurora_core::scheduler::{Scheduler, SchedulerEvent, BeamStatus};
use aurora_core::ast::{Beam, Run};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::sync::Arc;
use tokio::sync::mpsc;

fn make_beam(name: &str, deps: Vec<&str>, commands: Vec<&str>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: if commands.is_empty() { None } else {
            Some(Run {
                commands: commands.iter().map(|s| s.to_string()).collect(),
                executor: None,
            })
        },
    }
}

#[tokio::test]
async fn test_scheduler_simple() {
    let beams = vec![
        make_beam("a", vec![], vec!["echo a"]),
        make_beam("b", vec!["a"], vec!["echo b"]),
    ];
    let executor: Arc<dyn Executor> = Arc::new(LocalExecutor::new());
    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(beams, executor, tx, None, std::path::PathBuf::from("/tmp"));
    scheduler.run("b").await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    let success_beams: Vec<_> = events.iter()
        .filter_map(|e| if let SchedulerEvent::BeamCompleted { name, status: BeamStatus::Success { .. } } = e { Some(name.as_str()) } else { None })
        .collect();
    assert!(success_beams.contains(&"a"));
    assert!(success_beams.contains(&"b"));
    let a_pos = success_beams.iter().position(|&n| n == "a").unwrap();
    let b_pos = success_beams.iter().position(|&n| n == "b").unwrap();
    assert!(a_pos < b_pos);
}

#[tokio::test]
async fn test_scheduler_failed_cancels_dependents() {
    let beams = vec![
        make_beam("a", vec![], vec!["false"]),
        make_beam("b", vec!["a"], vec!["echo b"]),
    ];
    let executor: Arc<dyn Executor> = Arc::new(LocalExecutor::new());
    let (tx, mut rx) = mpsc::channel(32);
    Scheduler::new(beams, executor, tx, None, std::path::PathBuf::from("/tmp")).run("b").await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    let failed = events.iter().any(|e| matches!(e, SchedulerEvent::BeamCompleted { name, status: BeamStatus::Failed { .. } } if name == "a"));
    let cancelled = events.iter().any(|e| matches!(e, SchedulerEvent::BeamCompleted { name, status: BeamStatus::Cancelled } if name == "b"));
    assert!(failed, "a should have failed");
    assert!(cancelled, "b should be cancelled");
}
