//! Parallel beam executor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use aurora_core::{AuroraError, Beamfile, Result};
use tokio::sync::Semaphore;

use crate::cache::BuildCache;
use crate::dag::DependencyGraph;
use crate::runner::CommandRunner;
use crate::scheduler::Scheduler;

/// Executes beams based on the dependency graph.
pub struct Executor {
    /// The Beamfile being executed.
    beamfile: Beamfile,

    /// The scheduler for execution planning.
    scheduler: Scheduler,

    /// Command runner for executing shell commands.
    runner: CommandRunner,

    /// Build cache for skipping unchanged beams.
    cache: BuildCache,

    /// Working directory for execution.
    working_dir: PathBuf,

    /// Whether to use caching.
    use_cache: bool,

    /// Dry run mode (don't actually execute).
    dry_run: bool,
}

/// Execution report.
#[derive(Debug)]
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

impl Executor {
    /// Creates a new executor.
    pub fn new(
        beamfile: Beamfile,
        working_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        let working_dir = working_dir.into();
        let dag = DependencyGraph::from_beamfile(&beamfile)?;
        let scheduler = Scheduler::new(dag);
        let runner = CommandRunner::new(&working_dir);
        let cache = BuildCache::new(cache_dir)?;

        Ok(Self {
            beamfile,
            scheduler,
            runner,
            cache,
            working_dir,
            use_cache: true,
            dry_run: false,
        })
    }

    /// Enables or disables caching.
    pub fn with_cache(mut self, enabled: bool) -> Self {
        self.use_cache = enabled;
        self
    }

    /// Enables or disables dry run mode.
    pub fn with_dry_run(mut self, enabled: bool) -> Self {
        self.dry_run = enabled;
        self
    }

    /// Sets the maximum parallelism.
    pub fn with_max_parallelism(mut self, max: usize) -> Self {
        self.scheduler = self.scheduler.with_max_parallelism(max);
        self
    }

    /// Executes a target beam and all its dependencies.
    pub async fn execute(&mut self, target: &str) -> Result<ExecutionReport> {
        let start = std::time::Instant::now();
        let plan = self.scheduler.execution_plan(target)?;

        let mut report = ExecutionReport {
            executed: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
            duration_ms: 0,
        };

        let max_parallelism = self.scheduler.max_parallelism();
        let semaphore = Arc::new(Semaphore::new(max_parallelism));

        for level in &plan.levels {
            if level.beams.len() == 1 {
                // Sequential execution for single beam
                let beam_name = &level.beams[0];
                self.execute_beam(beam_name, &mut report).await?;
            } else {
                // Parallel execution for multiple beams
                // For simplicity, execute sequentially within parallel levels
                // A full implementation would use tokio::spawn with proper lifetimes
                for beam_name in &level.beams {
                    let _permit = semaphore.clone().acquire_owned().await.unwrap();
                    self.execute_beam(beam_name, &mut report).await?;
                }
            }

            // Stop if any beam failed
            if !report.failed.is_empty() {
                break;
            }
        }

        report.duration_ms = start.elapsed().as_millis() as u64;
        Ok(report)
    }

    /// Executes a single beam.
    async fn execute_beam(&mut self, beam_name: &str, report: &mut ExecutionReport) -> Result<()> {
        let beam = self
            .beamfile
            .get_beam(beam_name)
            .ok_or_else(|| AuroraError::BeamNotFound(beam_name.to_string()))?
            .clone();

        // Check cache
        if self.use_cache && self.cache.is_up_to_date(&beam, &self.working_dir) {
            report.skipped.push(beam_name.to_string());
            return Ok(());
        }

        // Check condition
        if let Some(ref condition) = beam.condition {
            if !self.evaluate_condition(condition).await {
                report.skipped.push(beam_name.to_string());
                return Ok(());
            }
        }

        if self.dry_run {
            report.executed.push(beam_name.to_string());
            return Ok(());
        }

        // Execute pre-hooks
        for hook in &beam.pre_hooks {
            let run_block = aurora_core::RunBlock::new(
                hook.commands
                    .iter()
                    .map(aurora_core::Command::new)
                    .collect(),
            );
            self.runner.execute_run_block(&run_block, &beam.env).await?;
        }

        // Execute main run block
        if let Some(ref run) = beam.run {
            if let Err(e) = self.runner.execute_run_block(run, &beam.env).await {
                report.failed.push((beam_name.to_string(), e.to_string()));
                return Err(e);
            }
        }

        // Execute post-hooks
        for hook in &beam.post_hooks {
            let run_block = aurora_core::RunBlock::new(
                hook.commands
                    .iter()
                    .map(aurora_core::Command::new)
                    .collect(),
            );
            self.runner.execute_run_block(&run_block, &beam.env).await?;
        }

        // Update cache
        if self.use_cache {
            self.cache.record(&beam, &self.working_dir)?;
        }

        report.executed.push(beam_name.to_string());
        Ok(())
    }

    /// Evaluates a condition.
    async fn evaluate_condition(&self, condition: &aurora_core::Condition) -> bool {
        match condition {
            aurora_core::Condition::FileExists(path) => self.working_dir.join(path).exists(),
            aurora_core::Condition::EnvSet(name) => std::env::var(name).is_ok(),
            aurora_core::Condition::EnvEquals { name, value } => {
                std::env::var(name).map(|v| v == *value).unwrap_or(false)
            }
            aurora_core::Condition::Command {
                run,
                expect_success,
            } => {
                let result = self
                    .runner
                    .execute_command(run, &self.working_dir, &HashMap::new())
                    .await;

                match result {
                    Ok(r) => (r.exit_code == 0) == *expect_success,
                    Err(_) => !expect_success,
                }
            }
            aurora_core::Condition::And(conditions) => {
                for c in conditions {
                    if !Box::pin(self.evaluate_condition(c)).await {
                        return false;
                    }
                }
                true
            }
            aurora_core::Condition::Or(conditions) => {
                for c in conditions {
                    if Box::pin(self.evaluate_condition(c)).await {
                        return true;
                    }
                }
                false
            }
            aurora_core::Condition::Not(condition) => {
                !Box::pin(self.evaluate_condition(condition)).await
            }
        }
    }
}
