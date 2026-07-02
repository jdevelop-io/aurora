use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent, SkipReason};
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

fn beam_with_skip_if(cmd: &str) -> Beam {
    Beam {
        name: "b".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        dir: None,
        skip_if: Some(cmd.to_string()),
        condition: None,
        run: Some(Run {
            commands: vec!["echo ran".to_string()],
            executor: None,
        }),
        allow_failure: false,
    }
}

async fn run_status(beam: Beam) -> BeamStatus {
    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(
        vec![beam],
        local_executors(),
        tx,
        None,
        std::path::PathBuf::from("/tmp"),
        HashMap::new(),
    );
    scheduler.run("b", &[]).await.unwrap();
    let mut status = None;
    while let Ok(e) = rx.try_recv() {
        if let SchedulerEvent::BeamCompleted { status: s, .. } = e {
            status = Some(s);
        }
    }
    status.expect("beam completed")
}

#[tokio::test]
async fn skip_if_success_skips_beam() {
    // The skip_if command succeeds (exit 0), so the beam is skipped.
    let status = run_status(beam_with_skip_if("true")).await;
    assert!(
        matches!(
            status,
            BeamStatus::Skipped {
                reason: SkipReason::SkipIf
            }
        ),
        "a succeeding skip_if should skip the beam, got {status:?}"
    );
}

#[tokio::test]
async fn skip_if_failure_runs_beam() {
    // The skip_if command fails (non-zero), so the beam runs.
    let status = run_status(beam_with_skip_if("false")).await;
    assert!(
        matches!(status, BeamStatus::Success { .. }),
        "a failing skip_if should let the beam run, got {status:?}"
    );
}
