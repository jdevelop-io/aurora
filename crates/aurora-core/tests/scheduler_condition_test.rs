use aurora_core::ast::{Beam, Condition, ConditionClause, ConditionOp, Run};
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

fn beam_with_condition(op: ConditionOp, shells: Vec<&str>) -> Beam {
    Beam {
        name: "b".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        dir: None,
        skip_if: None,
        condition: Some(Condition {
            op,
            clauses: shells
                .into_iter()
                .map(|s| ConditionClause::Shell(s.to_string()))
                .collect(),
        }),
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
async fn condition_all_false_skips_beam() {
    let status = run_status(beam_with_condition(ConditionOp::All, vec!["false"])).await;
    assert!(
        matches!(
            status,
            BeamStatus::Skipped {
                reason: SkipReason::ConditionNotMet
            }
        ),
        "unmet condition should skip, got {status:?}"
    );
}

#[tokio::test]
async fn condition_all_true_runs_beam() {
    let status = run_status(beam_with_condition(ConditionOp::All, vec!["true", "true"])).await;
    assert!(
        matches!(status, BeamStatus::Success { .. }),
        "met condition should run, got {status:?}"
    );
}

#[tokio::test]
async fn condition_any_one_true_runs_beam() {
    let status = run_status(beam_with_condition(ConditionOp::Any, vec!["false", "true"])).await;
    assert!(
        matches!(status, BeamStatus::Success { .. }),
        "any-satisfied condition should run, got {status:?}"
    );
}

#[tokio::test]
async fn condition_any_all_false_skips_beam() {
    let status = run_status(beam_with_condition(
        ConditionOp::Any,
        vec!["false", "false"],
    ))
    .await;
    assert!(
        matches!(
            status,
            BeamStatus::Skipped {
                reason: SkipReason::ConditionNotMet
            }
        ),
        "no clause satisfied should skip, got {status:?}"
    );
}
