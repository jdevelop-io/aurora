use async_trait::async_trait;
use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// An executor that panics mid-execution, simulating a bug in an executor
/// implementation (rather than a well-behaved `Err` return).
struct PanickingExecutor;

#[async_trait]
impl Executor for PanickingExecutor {
    fn name(&self) -> &str {
        "panicking"
    }

    async fn execute(&self, _input: ExecutionInput) -> anyhow::Result<ExecutionOutput> {
        panic!("boom");
    }
}

fn beam(name: &str, executor: Option<&str>, depends_on: Vec<&str>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: depends_on.into_iter().map(String::from).collect(),
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["echo unused".to_string()],
            executor: executor.map(|e| aurora_core::ast::ExecutorConfig {
                name: e.to_string(),
                config: HashMap::new(),
            }),
        }),
        allow_failure: false,
    }
}

/// A panicking beam task must not leave its dependents stuck: the run has to
/// terminate, report failure, and give both the panicked beam and its
/// downstream a terminal status.
#[tokio::test]
async fn panicked_beam_fails_and_cancels_dependents() {
    let mut executors: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    executors.insert("panicking".into(), Arc::new(PanickingExecutor));
    executors.insert(
        "local".into(),
        Arc::new(aurora_executor_local::LocalExecutor::new()),
    );

    let base = beam("base", Some("panicking"), vec![]);
    let dependent = beam("dependent", Some("local"), vec!["base"]);

    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(
        vec![base, dependent],
        executors,
        tx,
        None,
        std::path::PathBuf::from("/tmp"),
        HashMap::new(),
    );

    // Must not hang, and must report overall failure.
    let ok = scheduler.run("dependent", &[]).await.unwrap();
    assert!(!ok, "a panicked beam must make the run fail");

    let mut base_failed = false;
    let mut dependent_cancelled = false;
    let mut all_done_success = None;
    while let Ok(e) = rx.try_recv() {
        match e {
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Failed { .. },
            } if name == "base" => base_failed = true,
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Cancelled,
            } if name == "dependent" => dependent_cancelled = true,
            SchedulerEvent::AllDone { success } => all_done_success = Some(success),
            _ => {}
        }
    }

    assert!(base_failed, "the panicked beam must be reported as failed");
    assert!(
        dependent_cancelled,
        "the dependent must be cancelled, not left pending"
    );
    assert_eq!(all_done_success, Some(false));
}
