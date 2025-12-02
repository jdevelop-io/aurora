//! Parallel beam executor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use aurora_core::{AuroraError, Beamfile, Result};
use tokio::sync::{Mutex, RwLock, Semaphore};

use crate::cache::BuildCache;
use crate::dag::DependencyGraph;
use crate::runner::CommandRunner;
use crate::scheduler::Scheduler;

/// Callback for beam execution events.
pub type BeamCallback = Arc<dyn Fn(BeamEvent) + Send + Sync>;

/// Events emitted during beam execution.
#[derive(Debug, Clone)]
pub enum BeamEvent {
    /// Beam execution started.
    Started { name: String },
    /// Beam was skipped (cached or condition).
    Skipped { name: String, reason: SkipReason },
    /// Beam completed successfully.
    Completed { name: String, duration_ms: u64 },
    /// Beam failed.
    Failed { name: String, error: String },
    /// Command output (stdout or stderr).
    Output {
        name: String,
        line: String,
        is_stderr: bool,
    },
}

/// Reason why a beam was skipped.
#[derive(Debug, Clone)]
pub enum SkipReason {
    /// Beam is up-to-date (cached).
    Cached,
    /// Condition evaluated to false.
    ConditionFalse,
}

/// Shared state for parallel execution.
struct ExecutorState {
    /// The Beamfile being executed.
    beamfile: Beamfile,
    /// Command runner for executing shell commands.
    runner: CommandRunner,
    /// Build cache for skipping unchanged beams.
    cache: Mutex<BuildCache>,
    /// Working directory for execution.
    working_dir: PathBuf,
    /// Whether to use caching.
    use_cache: bool,
    /// Dry run mode (don't actually execute).
    dry_run: bool,
    /// Optional callback for events.
    callback: Option<BeamCallback>,
}

/// Executes beams based on the dependency graph.
pub struct Executor {
    /// Shared state wrapped in Arc for parallel access.
    state: Arc<ExecutorState>,
    /// The scheduler for execution planning.
    scheduler: Scheduler,
    /// Semaphore for limiting parallelism.
    semaphore: Arc<Semaphore>,
}

/// Execution report.
#[derive(Debug, Default)]
pub struct ExecutionReport {
    /// Beams that were executed.
    pub executed: Vec<String>,
    /// Beams that were skipped (cached).
    pub skipped: Vec<String>,
    /// Beams that failed.
    pub failed: Vec<(String, String)>,
    /// Total execution time in milliseconds.
    pub duration_ms: u64,
}

/// Thread-safe execution report for parallel execution.
#[derive(Debug)]
struct SharedReport {
    executed: Mutex<Vec<String>>,
    skipped: Mutex<Vec<String>>,
    failed: RwLock<Vec<(String, String)>>,
}

impl SharedReport {
    fn new() -> Self {
        Self {
            executed: Mutex::new(Vec::new()),
            skipped: Mutex::new(Vec::new()),
            failed: RwLock::new(Vec::new()),
        }
    }

    async fn add_executed(&self, name: String) {
        self.executed.lock().await.push(name);
    }

    async fn add_skipped(&self, name: String) {
        self.skipped.lock().await.push(name);
    }

    async fn add_failed(&self, name: String, error: String) {
        self.failed.write().await.push((name, error));
    }

    async fn has_failures(&self) -> bool {
        !self.failed.read().await.is_empty()
    }

    async fn into_report(self, duration_ms: u64) -> ExecutionReport {
        ExecutionReport {
            executed: self.executed.into_inner(),
            skipped: self.skipped.into_inner(),
            failed: self.failed.into_inner(),
            duration_ms,
        }
    }
}

impl Executor {
    /// Creates a new executor.
    pub fn new(
        beamfile: Beamfile,
        working_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        let working_dir = working_dir.into();
        let dag = DependencyGraph::from_beamfile(&beamfile)?;
        let max_parallelism = num_cpus::get();
        let scheduler = Scheduler::new(dag).with_max_parallelism(max_parallelism);
        let runner = CommandRunner::new(&working_dir);
        let cache = BuildCache::new(cache_dir)?;

        let state = Arc::new(ExecutorState {
            beamfile,
            runner,
            cache: Mutex::new(cache),
            working_dir,
            use_cache: true,
            dry_run: false,
            callback: None,
        });

        Ok(Self {
            state,
            scheduler,
            semaphore: Arc::new(Semaphore::new(max_parallelism)),
        })
    }

    /// Enables or disables caching.
    pub fn with_cache(mut self, enabled: bool) -> Self {
        Arc::get_mut(&mut self.state)
            .expect("Cannot modify state after cloning")
            .use_cache = enabled;
        self
    }

    /// Enables or disables dry run mode.
    pub fn with_dry_run(mut self, enabled: bool) -> Self {
        Arc::get_mut(&mut self.state)
            .expect("Cannot modify state after cloning")
            .dry_run = enabled;
        self
    }

    /// Sets the maximum parallelism.
    pub fn with_max_parallelism(mut self, max: usize) -> Self {
        let max = max.max(1);
        self.scheduler = self.scheduler.with_max_parallelism(max);
        self.semaphore = Arc::new(Semaphore::new(max));
        self
    }

    /// Sets a callback for beam events.
    pub fn with_callback(mut self, callback: BeamCallback) -> Self {
        Arc::get_mut(&mut self.state)
            .expect("Cannot modify state after cloning")
            .callback = Some(callback);
        self
    }

    /// Executes a target beam and all its dependencies.
    pub async fn execute(&self, target: &str) -> Result<ExecutionReport> {
        let start = std::time::Instant::now();
        let plan = self.scheduler.execution_plan(target)?;
        let report = Arc::new(SharedReport::new());

        for level in &plan.levels {
            // Check for failures before starting new level
            if report.has_failures().await {
                break;
            }

            if level.beams.len() == 1 {
                // Single beam - execute directly
                let beam_name = &level.beams[0];
                self.execute_beam_task(beam_name, report.clone()).await;
            } else {
                // Multiple beams - execute in parallel with tokio::spawn
                let mut handles = Vec::with_capacity(level.beams.len());

                for beam_name in &level.beams {
                    let state = self.state.clone();
                    let semaphore = self.semaphore.clone();
                    let report = report.clone();
                    let beam_name = beam_name.clone();

                    let handle = tokio::spawn(async move {
                        // Acquire semaphore permit to limit parallelism
                        let _permit = semaphore.acquire().await.unwrap();
                        execute_beam(&state, &beam_name, &report).await;
                    });

                    handles.push(handle);
                }

                // Wait for all beams in this level to complete
                for handle in handles {
                    let _ = handle.await;
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // Unwrap the Arc to get the owned SharedReport
        let shared_report = Arc::try_unwrap(report).expect("All tasks should be complete");

        Ok(shared_report.into_report(duration_ms).await)
    }

    /// Execute a beam task (used for single-beam levels).
    async fn execute_beam_task(&self, beam_name: &str, report: Arc<SharedReport>) {
        execute_beam(&self.state, beam_name, &report).await;
    }
}

/// Execute a single beam (standalone function for use with tokio::spawn).
async fn execute_beam(state: &ExecutorState, beam_name: &str, report: &SharedReport) {
    let result = execute_beam_inner(state, beam_name).await;

    match result {
        Ok(BeamResult::Executed) => {
            report.add_executed(beam_name.to_string()).await;
            emit_event(
                state,
                BeamEvent::Completed {
                    name: beam_name.to_string(),
                    duration_ms: 0, // TODO: track individual beam duration
                },
            );
        }
        Ok(BeamResult::Skipped(reason)) => {
            report.add_skipped(beam_name.to_string()).await;
            emit_event(
                state,
                BeamEvent::Skipped {
                    name: beam_name.to_string(),
                    reason,
                },
            );
        }
        Err(e) => {
            let error = e.to_string();
            report
                .add_failed(beam_name.to_string(), error.clone())
                .await;
            emit_event(
                state,
                BeamEvent::Failed {
                    name: beam_name.to_string(),
                    error,
                },
            );
        }
    }
}

/// Result of beam execution.
enum BeamResult {
    Executed,
    Skipped(SkipReason),
}

/// Inner beam execution logic.
async fn execute_beam_inner(state: &ExecutorState, beam_name: &str) -> Result<BeamResult> {
    let beam = state
        .beamfile
        .get_beam(beam_name)
        .ok_or_else(|| AuroraError::BeamNotFound(beam_name.to_string()))?
        .clone();

    emit_event(
        state,
        BeamEvent::Started {
            name: beam_name.to_string(),
        },
    );

    // Check cache
    if state.use_cache {
        let cache = state.cache.lock().await;
        if cache.is_up_to_date(&beam, &state.working_dir) {
            return Ok(BeamResult::Skipped(SkipReason::Cached));
        }
    }

    // Check condition
    if let Some(ref condition) = beam.condition {
        if !evaluate_condition(state, condition).await {
            return Ok(BeamResult::Skipped(SkipReason::ConditionFalse));
        }
    }

    if state.dry_run {
        return Ok(BeamResult::Executed);
    }

    // Execute pre-hooks
    for hook in &beam.pre_hooks {
        let run_block = aurora_core::RunBlock::new(
            hook.commands
                .iter()
                .map(aurora_core::Command::new)
                .collect(),
        );
        state
            .runner
            .execute_run_block(&run_block, &beam.env)
            .await?;
    }

    // Execute main run block
    if let Some(ref run) = beam.run {
        state.runner.execute_run_block(run, &beam.env).await?;
    }

    // Execute post-hooks
    for hook in &beam.post_hooks {
        let run_block = aurora_core::RunBlock::new(
            hook.commands
                .iter()
                .map(aurora_core::Command::new)
                .collect(),
        );
        state
            .runner
            .execute_run_block(&run_block, &beam.env)
            .await?;
    }

    // Update cache
    if state.use_cache {
        let mut cache = state.cache.lock().await;
        cache.record(&beam, &state.working_dir)?;
    }

    Ok(BeamResult::Executed)
}

/// Evaluates a condition.
async fn evaluate_condition(state: &ExecutorState, condition: &aurora_core::Condition) -> bool {
    match condition {
        aurora_core::Condition::FileExists(path) => state.working_dir.join(path).exists(),
        aurora_core::Condition::EnvSet(name) => std::env::var(name).is_ok(),
        aurora_core::Condition::EnvEquals { name, value } => {
            std::env::var(name).map(|v| v == *value).unwrap_or(false)
        }
        aurora_core::Condition::Command {
            run,
            expect_success,
        } => {
            let result = state
                .runner
                .execute_command(run, &state.working_dir, &HashMap::new())
                .await;

            match result {
                Ok(r) => (r.exit_code == 0) == *expect_success,
                Err(_) => !expect_success,
            }
        }
        aurora_core::Condition::And(conditions) => {
            for c in conditions {
                if !Box::pin(evaluate_condition(state, c)).await {
                    return false;
                }
            }
            true
        }
        aurora_core::Condition::Or(conditions) => {
            for c in conditions {
                if Box::pin(evaluate_condition(state, c)).await {
                    return true;
                }
            }
            false
        }
        aurora_core::Condition::Not(condition) => {
            !Box::pin(evaluate_condition(state, condition)).await
        }
    }
}

/// Emit an event to the callback if configured.
fn emit_event(state: &ExecutorState, event: BeamEvent) {
    if let Some(ref callback) = state.callback {
        callback(event);
    }
}

/// Builder for creating an Executor with custom configuration.
pub struct ExecutorBuilder {
    beamfile: Beamfile,
    working_dir: PathBuf,
    cache_dir: PathBuf,
    use_cache: bool,
    dry_run: bool,
    max_parallelism: Option<usize>,
    callback: Option<BeamCallback>,
}

impl ExecutorBuilder {
    /// Creates a new builder.
    pub fn new(beamfile: Beamfile, working_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            beamfile,
            working_dir,
            cache_dir,
            use_cache: true,
            dry_run: false,
            max_parallelism: None,
            callback: None,
        }
    }

    /// Enables or disables caching.
    pub fn cache(mut self, enabled: bool) -> Self {
        self.use_cache = enabled;
        self
    }

    /// Enables or disables dry run mode.
    pub fn dry_run(mut self, enabled: bool) -> Self {
        self.dry_run = enabled;
        self
    }

    /// Sets maximum parallelism.
    pub fn max_parallelism(mut self, max: usize) -> Self {
        self.max_parallelism = Some(max);
        self
    }

    /// Sets an event callback.
    pub fn callback(mut self, callback: BeamCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// Builds the executor.
    pub fn build(self) -> Result<Executor> {
        let mut executor = Executor::new(self.beamfile, self.working_dir, self.cache_dir)?;

        executor = executor.with_cache(self.use_cache);
        executor = executor.with_dry_run(self.dry_run);

        if let Some(max) = self.max_parallelism {
            executor = executor.with_max_parallelism(max);
        }

        if let Some(callback) = self.callback {
            executor = executor.with_callback(callback);
        }

        Ok(executor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::Beam;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_parallel_execution() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join(".aurora/cache");

        // Create a simple beamfile with parallel beams
        let mut beamfile = Beamfile::new(dir.path().join("Beamfile"));

        // Add independent beams that can run in parallel
        let beam1 = Beam::new("beam1").with_run(aurora_core::RunBlock::from_strings(vec![
            "echo beam1".to_string(),
        ]));
        let beam2 = Beam::new("beam2").with_run(aurora_core::RunBlock::from_strings(vec![
            "echo beam2".to_string(),
        ]));
        let beam3 = Beam::new("all")
            .with_depends_on(vec!["beam1".to_string(), "beam2".to_string()])
            .with_run(aurora_core::RunBlock::from_strings(vec![
                "echo all".to_string(),
            ]));

        beamfile.add_beam(beam1);
        beamfile.add_beam(beam2);
        beamfile.add_beam(beam3);

        let executor = Executor::new(beamfile, dir.path(), cache_dir)
            .unwrap()
            .with_cache(false);

        let report = executor.execute("all").await.unwrap();

        assert_eq!(report.executed.len(), 3);
        assert!(report.failed.is_empty());
    }

    #[tokio::test]
    async fn test_callback_events() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join(".aurora/cache");

        let mut beamfile = Beamfile::new(dir.path().join("Beamfile"));
        let beam = Beam::new("test").with_run(aurora_core::RunBlock::from_strings(vec![
            "echo test".to_string(),
        ]));
        beamfile.add_beam(beam);

        let event_count = Arc::new(AtomicUsize::new(0));
        let event_count_clone = event_count.clone();

        let callback: BeamCallback = Arc::new(move |_event| {
            event_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let executor = Executor::new(beamfile, dir.path(), cache_dir)
            .unwrap()
            .with_cache(false)
            .with_callback(callback);

        let _ = executor.execute("test").await.unwrap();

        // Should have at least Started and Completed events
        assert!(event_count.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn test_semaphore_limits_parallelism() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join(".aurora/cache");

        let mut beamfile = Beamfile::new(dir.path().join("Beamfile"));

        // Create many independent beams
        for i in 0..10 {
            let beam = Beam::new(format!("beam{}", i)).with_run(
                aurora_core::RunBlock::from_strings(vec!["echo test".to_string()]),
            );
            beamfile.add_beam(beam);
        }

        // Add a beam that depends on all of them
        let all = Beam::new("all").with_depends_on((0..10).map(|i| format!("beam{}", i)).collect());
        beamfile.add_beam(all);

        // Limit parallelism to 2
        let executor = Executor::new(beamfile, dir.path(), cache_dir)
            .unwrap()
            .with_cache(false)
            .with_max_parallelism(2);

        let report = executor.execute("all").await.unwrap();

        // All beams should execute successfully (10 independent + "all" = 11)
        assert_eq!(report.executed.len(), 11);
        assert!(report.failed.is_empty());
    }
}
