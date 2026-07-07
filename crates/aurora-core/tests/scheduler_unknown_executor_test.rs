use aurora_core::ast::{Beam, ExecutorConfig, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert(
        "local".into(),
        Arc::new(aurora_executor_local::LocalExecutor::new()),
    );
    m
}

fn beam_with_executor(name: &str, executor_name: &str) -> Beam {
    Beam {
        name: name.to_string(),
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
            commands: vec!["echo should-not-run".to_string()],
            executor: Some(ExecutorConfig {
                name: executor_name.to_string(),
                config: HashMap::new(),
            }),
        }),
        allow_failure: false,
    }
}

/// A beam that explicitly names an executor which is not registered must fail
/// loudly rather than silently running its commands on the host `local`
/// executor: a Beamfile is untrusted, and a beam meant to run sandboxed must
/// not be downgraded to local execution because of a typo.
#[tokio::test]
async fn unknown_named_executor_fails_the_beam() {
    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(
        vec![beam_with_executor("b", "dcoker")],
        local_executors(),
        tx,
        None,
        std::path::PathBuf::from("/tmp"),
        HashMap::new(),
    );
    let overall_success = scheduler.run("b", &[]).await.unwrap();

    let mut failed = false;
    let mut error_line = None;
    while let Ok(e) = rx.try_recv() {
        match e {
            SchedulerEvent::BeamCompleted {
                status: BeamStatus::Failed { .. },
                ..
            } => failed = true,
            SchedulerEvent::BeamOutput {
                line, is_stderr, ..
            } if is_stderr => error_line = Some(line),
            _ => {}
        }
    }

    assert!(
        !overall_success,
        "the run must not be reported as successful"
    );
    assert!(failed, "the beam should be reported as failed");
    let line = error_line.expect("an error line should be emitted on stderr");
    assert!(
        line.contains("dcoker"),
        "error should name the unknown executor, got: {line}"
    );
}
