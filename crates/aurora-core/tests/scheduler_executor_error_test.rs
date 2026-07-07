use anyhow::anyhow;
use async_trait::async_trait;
use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// An executor that always fails to launch (e.g. unreachable Docker daemon).
struct FailingExecutor;

#[async_trait]
impl Executor for FailingExecutor {
    fn name(&self) -> &str {
        "failing"
    }

    async fn execute(&self, _input: ExecutionInput) -> anyhow::Result<ExecutionOutput> {
        Err(anyhow!("could not reach the daemon"))
    }
}

/// When an executor returns an error (as opposed to a non-zero exit code),
/// the scheduler must surface the message instead of failing with an opaque
/// exit code -1.
#[tokio::test]
async fn executor_error_is_surfaced_to_output() {
    let mut executors: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    executors.insert("failing".into(), Arc::new(FailingExecutor));
    // A `local` fallback must exist for the scheduler to spawn.
    executors.insert(
        "local".into(),
        Arc::new(aurora_executor_local::LocalExecutor::new()),
    );

    let beam = Beam {
        name: "b".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        variables: vec![],
        args: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["echo unused".to_string()],
            executor: Some(aurora_core::ast::ExecutorConfig {
                name: "failing".to_string(),
                config: HashMap::new(),
            }),
        }),
        allow_failure: false,
    };

    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(
        vec![beam],
        executors,
        tx,
        None,
        std::path::PathBuf::from("/tmp"),
        HashMap::new(),
    );
    scheduler.run("b", &[]).await.unwrap();

    let mut error_line = None;
    let mut failed = false;
    while let Ok(e) = rx.try_recv() {
        match e {
            SchedulerEvent::BeamOutput {
                line, is_stderr, ..
            } if is_stderr => {
                error_line = Some(line);
            }
            SchedulerEvent::BeamCompleted {
                status: BeamStatus::Failed { .. },
                ..
            } => failed = true,
            _ => {}
        }
    }

    assert!(failed, "the beam should be reported as failed");
    let line = error_line.expect("an error line should be emitted on stderr");
    assert!(
        line.contains("could not reach the daemon"),
        "error message should be surfaced, got: {line}"
    );
}
