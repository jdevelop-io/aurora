use crate::ast::{Beam, Condition, ConditionClause, ConditionOp, Run};
use crate::cache::BeamCache;
use crate::dag::BeamGraph;
use anyhow::Result;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub enum BeamStatus {
    Pending,
    Running,
    Success { duration: Duration, cached: bool },
    Skipped { reason: SkipReason },
    Failed { exit_code: i32, duration: Duration },
    FailedAllowed { exit_code: i32, duration: Duration },
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    /// Inputs unchanged and outputs present: cached result replayed.
    Cached,
    /// The beam's `skip_if` command succeeded.
    SkipIf,
    /// The beam's `condition { }` block evaluated to false.
    ConditionNotMet,
}

#[derive(Debug)]
pub enum SchedulerEvent {
    BeamStarted {
        name: String,
    },
    BeamCompleted {
        name: String,
        status: BeamStatus,
    },
    BeamOutput {
        name: String,
        line: String,
        is_stderr: bool,
    },
    AllDone {
        success: bool,
    },
}

/// Outcome of a beam task, used to drive downstream scheduling.
enum BeamOutcome {
    /// Counts as a success (actual success, skip, cache hit, or tolerated failure).
    Ok,
    /// Non-tolerated failure.
    Failed,
    /// Cancelled by the user.
    Cancelled,
}

pub struct Scheduler {
    beams: HashMap<String, Beam>,
    executors: HashMap<String, Arc<dyn Executor>>,
    tx: mpsc::Sender<SchedulerEvent>,
    max_parallelism: Option<usize>,
    cache: Arc<BeamCache>,
    cache_enabled: bool,
    working_dir: PathBuf,
    env: HashMap<String, String>,
}

impl Scheduler {
    pub fn new(
        beams: Vec<Beam>,
        executors: HashMap<String, Arc<dyn Executor>>,
        tx: mpsc::Sender<SchedulerEvent>,
        max_parallelism: Option<usize>,
        working_dir: PathBuf,
        env: HashMap<String, String>,
    ) -> Self {
        let cache = Arc::new(BeamCache::new(working_dir.join(".aurora/cache")));
        Self {
            beams: beams.into_iter().map(|b| (b.name.clone(), b)).collect(),
            executors,
            tx,
            max_parallelism,
            cache,
            cache_enabled: true,
            working_dir,
            env,
        }
    }

    /// Disables the cache for this run: no cache hit is honored and no result
    /// is persisted. Backs the `--no-cache` CLI flag.
    pub fn without_cache(mut self) -> Self {
        self.cache_enabled = false;
        self
    }

    /// Non-cancellable variant, the historical signature: delegates with a
    /// silent channel (the sender stays alive for the whole run, so there is
    /// never any cancellation).
    pub async fn run(self, root: &str, pre_success: &[String]) -> Result<bool> {
        let (_cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
        self.run_cancellable(root, pre_success, cancel_rx).await
    }

    pub async fn run_cancellable(
        self,
        root: &str,
        pre_success: &[String],
        mut cancel_rx: mpsc::UnboundedReceiver<String>,
    ) -> Result<bool> {
        let deps: Vec<(String, Vec<String>)> = self
            .beams
            .values()
            .map(|b| (b.name.clone(), b.depends_on.clone()))
            .collect();
        let graph = BeamGraph::from_deps(deps)?;

        let nodes: HashSet<String> = graph
            .execution_levels(root)?
            .into_iter()
            .flatten()
            .collect();

        let semaphore = self
            .max_parallelism
            .map(|n| Arc::new(Semaphore::new(n.max(1))));
        let mut overall_success = true;

        let pre: HashSet<&String> = pre_success.iter().collect();

        let mut remaining: HashMap<String, usize> = HashMap::new();
        for n in &nodes {
            let in_degree = graph
                .direct_dependencies(n)
                .into_iter()
                .filter(|d| nodes.contains(d) && !pre.contains(d))
                .count();
            remaining.insert(n.clone(), in_degree);
        }

        let mut cancelled: HashSet<String> = HashSet::new();
        let mut spawned: HashSet<String> = HashSet::new();
        let mut set: JoinSet<(String, BeamOutcome)> = JoinSet::new();
        // One cancellation sender per spawned beam: receiving it inside the
        // task makes the `select!` win and triggers the cancellation.
        let mut cancels: HashMap<String, oneshot::Sender<()>> = HashMap::new();
        // Maps each running task's id to its beam name, so a panicked task
        // (whose return value is lost) can still be attributed to its beam.
        let mut task_names: HashMap<tokio::task::Id, String> = HashMap::new();

        for n in &nodes {
            if pre.contains(n) {
                continue;
            }
            if remaining[n] == 0 {
                let (cancel_tx, id) = self.spawn_beam(&mut set, &semaphore, n);
                cancels.insert(n.clone(), cancel_tx);
                task_names.insert(id, n.clone());
                spawned.insert(n.clone());
            }
        }

        loop {
            tokio::select! {
                joined = set.join_next_with_id() => {
                    let Some(result) = joined else { break };
                    let (name, outcome) = match result {
                        Ok((id, pair)) => {
                            task_names.remove(&id);
                            pair
                        }
                        Err(join_err) => {
                            // Panicked (or aborted) task: it never emitted its
                            // terminal event. Recover its beam name to report it
                            // as failed and cancel its dependents, so nothing is
                            // left stuck Pending.
                            overall_success = false;
                            let Some(name) = task_names.remove(&join_err.id()) else {
                                continue;
                            };
                            let _ = self
                                .tx
                                .send(SchedulerEvent::BeamCompleted {
                                    name: name.clone(),
                                    status: BeamStatus::Failed {
                                        exit_code: -1,
                                        duration: Duration::ZERO,
                                    },
                                })
                                .await;
                            (name, BeamOutcome::Failed)
                        }
                    };

                    match outcome {
                        BeamOutcome::Ok => {
                            // Success: unblock the direct dependents.
                            for dep in graph.direct_dependents(&name) {
                                if !nodes.contains(&dep) || cancelled.contains(&dep) || spawned.contains(&dep) || pre.contains(&dep) {
                                    continue;
                                }
                                if let Some(r) = remaining.get_mut(&dep) {
                                    *r = r.saturating_sub(1);
                                    if *r == 0 {
                                        let (cancel_tx, id) =
                                            self.spawn_beam(&mut set, &semaphore, &dep);
                                        cancels.insert(dep.clone(), cancel_tx);
                                        task_names.insert(id, dep.clone());
                                        spawned.insert(dep);
                                    }
                                }
                            }
                        }
                        BeamOutcome::Failed | BeamOutcome::Cancelled => {
                            overall_success = false;
                            // Cancel the whole downstream closure: these beams
                            // will never reach an in-degree of zero, so we emit
                            // their Cancelled once.
                            for dep in graph.transitive_dependents(&name) {
                                if nodes.contains(&dep) && !spawned.contains(&dep) && cancelled.insert(dep.clone()) {
                                    let _ = self.tx.send(SchedulerEvent::BeamCompleted {
                                        name: dep,
                                        status: BeamStatus::Cancelled,
                                    }).await;
                                }
                            }
                        }
                    }
                }
                Some(name) = cancel_rx.recv() => {
                    // Cancellation request from the TUI: trigger the beam's
                    // oneshot if it is running. Ignored if it has already finished.
                    if let Some(s) = cancels.remove(&name) {
                        let _ = s.send(());
                    }
                }
            }
        }

        let _ = self
            .tx
            .send(SchedulerEvent::AllDone {
                success: overall_success,
            })
            .await;
        Ok(overall_success)
    }

    /// Selects the executor for a beam from its `run.executor` name, falling
    /// back to `local`. `local` is registered by the composition root, so its
    /// absence is a programming error rather than a runtime condition.
    fn resolve_executor(&self, beam: &Beam) -> Arc<dyn Executor> {
        let executor_name = beam
            .run
            .as_ref()
            .and_then(|r| r.executor.as_ref())
            .map(|e| e.name.as_str())
            .unwrap_or("local");
        self.executors
            .get(executor_name)
            .or_else(|| self.executors.get("local"))
            .cloned()
            .expect("no local executor registered")
    }

    /// Spawns a beam task and returns its cancellation sender together with the
    /// spawned task's id (used to attribute a panicked task to its beam).
    fn spawn_beam(
        &self,
        set: &mut JoinSet<(String, BeamOutcome)>,
        semaphore: &Option<Arc<Semaphore>>,
        beam_name: &str,
    ) -> (oneshot::Sender<()>, tokio::task::Id) {
        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

        let beam = self.beams[beam_name].clone();
        let env = self.env.clone();
        let executor = self.resolve_executor(&beam);
        let tx = self.tx.clone();
        let sem = semaphore.clone();
        let cache = self.cache.clone();
        let cache_enabled = self.cache_enabled;
        let working_dir = self.working_dir.clone();

        let handle = set.spawn(async move {
            let _permit = match sem {
                Some(s) => Some(s.acquire_owned().await.unwrap()),
                None => None,
            };

            let _ = tx
                .send(SchedulerEvent::BeamStarted {
                    name: beam.name.clone(),
                })
                .await;

            // A beam without a `run` block is a pure aggregation node: it
            // succeeds immediately once its dependencies are done.
            if beam.run.is_none() {
                let _ = tx
                    .send(SchedulerEvent::BeamCompleted {
                        name: beam.name.clone(),
                        status: BeamStatus::Success {
                            duration: Duration::ZERO,
                            cached: false,
                        },
                    })
                    .await;
                return (beam.name, BeamOutcome::Ok);
            }

            // Gating: skip when `skip_if` succeeds, then when `condition` is
            // not met. Either way the beam counts as a success for scheduling.
            if let Some(reason) = gate_skip_reason(&beam, &working_dir, &env).await {
                let _ = tx
                    .send(SchedulerEvent::BeamCompleted {
                        name: beam.name.clone(),
                        status: BeamStatus::Skipped { reason },
                    })
                    .await;
                return (beam.name, BeamOutcome::Ok);
            }

            // Cache: on a valid hit, replay the recorded output and skip.
            let inputs_hash = if cache_enabled && !beam.inputs.is_empty() {
                cache.hash_inputs_at(&working_dir, &beam.inputs).ok().flatten()
            } else {
                None
            };
            if let Some(ref hash) = inputs_hash {
                if cache.is_valid(&beam.name, hash, &beam.outputs, &working_dir) {
                    replay_cached_output(&cache, &beam.name, &tx).await;
                    let _ = tx
                        .send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status: BeamStatus::Skipped {
                                reason: SkipReason::Cached,
                            },
                        })
                        .await;
                    return (beam.name, BeamOutcome::Ok);
                }
            }

            // Execute, streaming output live and racing against cancellation.
            let run = beam.run.as_ref().unwrap();
            let (out_tx, fwd_handle) = spawn_output_forwarder(tx.clone(), beam.name.clone());
            let input = ExecutionInput {
                commands: run.commands.clone(),
                env,
                working_dir: working_dir.clone(),
                config: build_executor_config(run),
                output_tx: Some(out_tx),
            };

            let start = Instant::now();
            // Race between the execution and a cancellation request. If the
            // cancellation wins, the `executor.execute(input)` future is
            // dropped: its child process is killed (kill_on_drop) and we emit
            // Cancelled.
            let result = tokio::select! {
                r = executor.execute(input) => r,
                _ = &mut cancel_rx => {
                    let _ = tx.send(SchedulerEvent::BeamCompleted {
                        name: beam.name.clone(),
                        status: BeamStatus::Cancelled,
                    }).await;
                    let _ = fwd_handle.await;
                    // A cancelled `allow_failure` beam is treated as a tolerated
                    // failure: its displayed status stays Cancelled, but for
                    // scheduling purposes it counts as a success (dependents
                    // unblocked, overall run not failed). Otherwise, the
                    // cancellation is propagated.
                    let outcome = if beam.allow_failure {
                        BeamOutcome::Ok
                    } else {
                        BeamOutcome::Cancelled
                    };
                    return (beam.name, outcome);
                }
            };
            let (stdout_lines, stderr_lines) = fwd_handle.await.unwrap_or_default();
            let duration = start.elapsed();

            // Side effects tied to the outcome: persist the cache on success,
            // surface the error message on failure to spawn.
            match &result {
                Ok(output) if output.success() => {
                    if let Some(ref hash) = inputs_hash {
                        let _ =
                            cache.save_with_logs(&beam.name, hash, &stdout_lines, &stderr_lines);
                    }
                }
                Err(e) => {
                    // Surface the executor error instead of dropping it: an
                    // unreachable Docker daemon, a missing image or a rejected
                    // volume would otherwise fail with an opaque exit code -1.
                    let _ = tx
                        .send(SchedulerEvent::BeamOutput {
                            name: beam.name.clone(),
                            line: format!("aurora: executor error: {e:#}"),
                            is_stderr: true,
                        })
                        .await;
                }
                _ => {}
            }

            let (status, outcome) = classify_execution(&result, beam.allow_failure, duration);
            let _ = tx
                .send(SchedulerEvent::BeamCompleted {
                    name: beam.name.clone(),
                    status,
                })
                .await;
            (beam.name, outcome)
        });

        (cancel_tx, handle.id())
    }
}

/// Runs a gating shell command and reports whether it succeeded (exit 0). A
/// command that fails to launch counts as "not succeeded".
async fn run_gate_command(cmd: &str, working_dir: &Path, env: &HashMap<String, String>) -> bool {
    tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(working_dir)
        .env_clear()
        .envs(env)
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Evaluates a `condition { }` block: true when the beam should run (all/any
/// of the shell clauses succeed).
async fn condition_met(
    condition: &Condition,
    working_dir: &Path,
    env: &HashMap<String, String>,
) -> bool {
    let mut clause_results = Vec::with_capacity(condition.clauses.len());
    for ConditionClause::Shell(cmd) in &condition.clauses {
        clause_results.push(run_gate_command(cmd, working_dir, env).await);
    }
    match condition.op {
        ConditionOp::All => clause_results.iter().all(|&ok| ok),
        ConditionOp::Any => clause_results.iter().any(|&ok| ok),
    }
}

/// Evaluates the beam's gates (`skip_if`, then `condition`) and returns the
/// reason to skip, or `None` when the beam should run.
async fn gate_skip_reason(
    beam: &Beam,
    working_dir: &Path,
    env: &HashMap<String, String>,
) -> Option<SkipReason> {
    if let Some(cond) = &beam.skip_if {
        if run_gate_command(cond, working_dir, env).await {
            return Some(SkipReason::SkipIf);
        }
    }
    if let Some(condition) = &beam.condition {
        if !condition_met(condition, working_dir, env).await {
            return Some(SkipReason::ConditionNotMet);
        }
    }
    None
}

/// Replays the stdout then stderr recorded in a cache entry as live output.
async fn replay_cached_output(
    cache: &BeamCache,
    beam_name: &str,
    tx: &mpsc::Sender<SchedulerEvent>,
) {
    let (stdout, stderr) = cache.load_logs(beam_name);
    for (lines, is_stderr) in [(stdout, false), (stderr, true)] {
        for line in lines {
            let _ = tx
                .send(SchedulerEvent::BeamOutput {
                    name: beam_name.to_string(),
                    line,
                    is_stderr,
                })
                .await;
        }
    }
}

/// The sender handed to an executor for its live output, paired with the task
/// that forwards those lines and collects `(stdout_lines, stderr_lines)`.
type OutputForwarder = (
    mpsc::Sender<(String, bool)>,
    tokio::task::JoinHandle<(Vec<String>, Vec<String>)>,
);

/// Spawns the task that forwards executor output lines to the scheduler
/// channel while accumulating them (to persist in the cache). Returns the
/// sender to hand to the executor and the join handle yielding
/// `(stdout_lines, stderr_lines)`.
fn spawn_output_forwarder(tx: mpsc::Sender<SchedulerEvent>, beam_name: String) -> OutputForwarder {
    let (out_tx, mut out_rx) = mpsc::channel::<(String, bool)>(256);
    let handle = tokio::spawn(async move {
        let mut stdout_lines: Vec<String> = vec![];
        let mut stderr_lines: Vec<String> = vec![];
        while let Some((line, is_stderr)) = out_rx.recv().await {
            let _ = tx
                .send(SchedulerEvent::BeamOutput {
                    name: beam_name.clone(),
                    line: line.clone(),
                    is_stderr,
                })
                .await;
            if is_stderr {
                stderr_lines.push(line);
            } else {
                stdout_lines.push(line);
            }
        }
        (stdout_lines, stderr_lines)
    });
    (out_tx, handle)
}

/// Turns a beam's executor config (string map) into the JSON value passed to
/// the executor.
fn build_executor_config(run: &Run) -> serde_json::Value {
    run.executor
        .as_ref()
        .map(|e| {
            let map: serde_json::Map<String, serde_json::Value> = e
                .config
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            serde_json::Value::Object(map)
        })
        .unwrap_or(serde_json::json!({}))
}

/// Maps an executor result to the beam's final display status and its
/// scheduling outcome (whether dependents are unblocked). `allow_failure`
/// downgrades a failure to a tolerated one that still counts as success.
fn classify_execution(
    result: &Result<ExecutionOutput>,
    allow_failure: bool,
    duration: Duration,
) -> (BeamStatus, BeamOutcome) {
    let exit_code = match result {
        Ok(output) if output.success() => {
            return (
                BeamStatus::Success {
                    duration,
                    cached: false,
                },
                BeamOutcome::Ok,
            );
        }
        Ok(output) => output.exit_code,
        Err(_) => -1,
    };
    if allow_failure {
        (
            BeamStatus::FailedAllowed {
                exit_code,
                duration,
            },
            BeamOutcome::Ok,
        )
    } else {
        (
            BeamStatus::Failed {
                exit_code,
                duration,
            },
            BeamOutcome::Failed,
        )
    }
}
