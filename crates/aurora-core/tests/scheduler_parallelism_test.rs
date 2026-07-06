use anyhow::Result;
use async_trait::async_trait;
use aurora_core::ast::{Beam, ExecutorConfig, Run};
use aurora_core::scheduler::Scheduler;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Executor that records the peak number of concurrent executions.
struct CountingExecutor {
    current: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

#[async_trait]
impl Executor for CountingExecutor {
    fn name(&self) -> &str {
        "counter"
    }

    async fn execute(&self, _input: ExecutionInput) -> Result<ExecutionOutput> {
        let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        // Hold the slot long enough for siblings to overlap if allowed to.
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.current.fetch_sub(1, Ordering::SeqCst);
        Ok(ExecutionOutput {
            exit_code: 0,
            stdout: vec![],
            stderr: vec![],
        })
    }
}

/// Aggregator root depending on `n` independent counter beams.
fn fan_out_beams(n: usize) -> Vec<Beam> {
    let mut beams: Vec<Beam> = (0..n)
        .map(|i| Beam {
            name: format!("b{i}"),
            description: None,
            depends_on: vec![],
            inputs: vec![],
            outputs: vec![],
            variables: vec![],
            dir: None,
            skip_if: None,
            condition: None,
            run: Some(Run {
                commands: vec!["noop".to_string()],
                executor: Some(ExecutorConfig {
                    name: "counter".to_string(),
                    config: HashMap::new(),
                }),
            }),
            allow_failure: false,
        })
        .collect();
    beams.push(Beam {
        name: "all".to_string(),
        description: None,
        depends_on: (0..n).map(|i| format!("b{i}")).collect(),
        inputs: vec![],
        outputs: vec![],
        variables: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: None,
        allow_failure: false,
    });
    beams
}

async fn run_peak(n: usize, max_parallelism: Option<usize>) -> usize {
    let current = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut executors: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    executors.insert(
        "counter".into(),
        Arc::new(CountingExecutor {
            current: current.clone(),
            peak: peak.clone(),
        }),
    );
    executors.insert("local".into(), Arc::new(LocalExecutor::new()));

    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        fan_out_beams(n),
        executors,
        tx,
        max_parallelism,
        std::path::PathBuf::from("/tmp"),
        HashMap::new(),
    );
    scheduler.run("all", &[]).await.unwrap();
    while rx.try_recv().is_ok() {}
    peak.load(Ordering::SeqCst)
}

#[tokio::test]
async fn max_parallelism_caps_concurrent_beams() {
    let peak = run_peak(6, Some(2)).await;
    assert!(peak <= 2, "max_parallelism=2 exceeded, peak was {peak}");
    assert!(peak >= 1, "nothing ran");
}

#[tokio::test]
async fn unbounded_parallelism_runs_beams_concurrently() {
    let peak = run_peak(6, None).await;
    assert!(
        peak >= 3,
        "independent beams should overlap without a cap, peak was {peak}"
    );
}
