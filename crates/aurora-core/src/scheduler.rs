use crate::ast::{Beam, Condition, ConditionClause, ConditionOp, Run};
use crate::cache::{BeamCache, BeamDefinition};
use crate::dag::BeamGraph;
use anyhow::Result;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinSet;

// The event/status contract lives in `crate::events`. Re-exported here so the
// scheduler's long-standing `scheduler::{SchedulerEvent, BeamStatus, ...}` path
// keeps working.
pub use crate::events::{BeamStatus, SchedulerEvent, SkipReason};

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
    /// The evaluated `environment {}` block as declared by the Beamfile, folded
    /// into every beam's cache key. A subset of `env`, which also carries the
    /// ambient allowlisted variables: those are machine context and must stay
    /// out of the key (see [`BeamDefinition::env`]).
    declared_env: BTreeMap<String, String>,
    /// Fires when the whole run must stop (Ctrl-C, SIGTERM). Distinct from the
    /// per-beam cancellation channel: that one targets a named beam, this one
    /// tears the run down.
    shutdown: Option<oneshot::Receiver<()>>,
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
            declared_env: BTreeMap::new(),
            shutdown: None,
        }
    }

    /// Arms a shutdown signal for this run: when `shutdown` fires, every running
    /// beam is cancelled and no further beam is spawned.
    ///
    /// Beams run in their own process group, so they never see the terminal's
    /// SIGINT. Without an explicit teardown the host process would die on the
    /// default disposition, its `Drop`-based process-group cleanup would not run,
    /// and the commands would outlive the run that started them.
    pub fn with_shutdown(mut self, shutdown: oneshot::Receiver<()>) -> Self {
        self.shutdown = Some(shutdown);
        self
    }

    /// Records the evaluated `environment {}` block so it takes part in every
    /// beam's cache key.
    ///
    /// A declared value feeds the commands without appearing in them (a
    /// `shell("git rev-parse HEAD")` sha, a branch name), so when it changes the
    /// beam's result changes and its entry must not be reused. Only the
    /// Beamfile-declared variables belong here, never the ambient allowlisted
    /// ones (see [`BeamDefinition::env`]).
    pub fn with_declared_env(mut self, declared_env: BTreeMap<String, String>) -> Self {
        self.declared_env = declared_env;
        self
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
        mut self,
        root: &str,
        pre_success: &[String],
        mut cancel_rx: mpsc::UnboundedReceiver<String>,
    ) -> Result<bool> {
        let mut shutdown = self.shutdown.take();
        let deps: Vec<(String, Vec<String>)> = self
            .beams
            .values()
            .map(|b| (b.name.clone(), b.depends_on.clone()))
            .collect();
        let graph = BeamGraph::from_deps(deps)?;

        // The scheduler is event-driven (in-degree based), so it only needs the
        // set of beams in the target's closure, not any level grouping. Cycles
        // are already rejected by `from_deps`.
        let nodes: HashSet<String> = graph.transitive_deps(root).into_iter().collect();

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

        let mut run = RunLoop::new(remaining);

        // Seed the loop with every beam that is ready from the start
        // (in-degree zero and not already satisfied by a previous run).
        for n in &nodes {
            if !pre.contains(n) && run.remaining[n] == 0 {
                self.spawn_and_track(&mut run, &semaphore, n);
            }
        }

        loop {
            tokio::select! {
                joined = run.set.join_next_with_id() => {
                    let Some(result) = joined else { break };
                    let (name, outcome) = match result {
                        Ok((id, pair)) => {
                            run.task_names.remove(&id);
                            pair
                        }
                        Err(join_err) => {
                            // Panicked (or aborted) task: it never emitted its
                            // terminal event. Recover its beam name to report it
                            // as failed and cancel its dependents, so nothing is
                            // left stuck Pending.
                            overall_success = false;
                            let Some(name) = run.task_names.remove(&join_err.id()) else {
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

                    // The beam is done: drop its cancellation sender so the map
                    // does not retain entries for finished beams.
                    run.cancels.remove(&name);

                    match outcome {
                        BeamOutcome::Ok => {
                            self.unblock_dependents(&mut run, &graph, &name, &nodes, &pre, &semaphore);
                        }
                        BeamOutcome::Failed | BeamOutcome::Cancelled => {
                            overall_success = false;
                            self.cancel_dependents(&mut run, &graph, &name, &nodes).await;
                        }
                    }
                }
                Some(name) = cancel_rx.recv() => {
                    // Cancellation request from the TUI: trigger the beam's
                    // oneshot if it is running. Ignored if it has already finished.
                    if let Some(s) = run.cancels.remove(&name) {
                        let _ = s.send(());
                    }
                }
                _ = wait_for_shutdown(&mut shutdown), if !run.shutting_down => {
                    // Tear the run down: cancel every running beam, and stop
                    // spawning. A torn-down run never completed, so it must
                    // report failure regardless of how its in-flight beams
                    // resolve. Without this, a running `allow_failure` beam (or
                    // a beam that finishes in the same instant) returns Ok, its
                    // suppressed dependents leave `overall_success` untouched,
                    // and the aborted run would falsely report success.
                    run.shutting_down = true;
                    overall_success = false;
                    for (_, cancel) in run.cancels.drain() {
                        let _ = cancel.send(());
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

    /// Spawns a beam and records its cancellation handle, task id and spawned
    /// mark in one place, so the three bookkeeping insertions cannot drift
    /// between the initial seed and the unblock path.
    ///
    /// A run that is shutting down spawns nothing more: the point of the
    /// teardown is to stop starting work, not just to stop the work in flight.
    fn spawn_and_track(&self, run: &mut RunLoop, semaphore: &Option<Arc<Semaphore>>, name: &str) {
        if run.shutting_down {
            return;
        }
        let (cancel_tx, id) = self.spawn_beam(&mut run.set, semaphore, name);
        run.cancels.insert(name.to_string(), cancel_tx);
        run.task_names.insert(id, name.to_string());
        run.spawned.insert(name.to_string());
    }

    /// A beam succeeded: decrement each not-yet-handled direct dependent's
    /// in-degree and spawn the ones that just reached zero.
    fn unblock_dependents(
        &self,
        run: &mut RunLoop,
        graph: &BeamGraph,
        name: &str,
        nodes: &HashSet<String>,
        pre: &HashSet<&String>,
        semaphore: &Option<Arc<Semaphore>>,
    ) {
        for dep in graph.direct_dependents(name) {
            if !nodes.contains(&dep)
                || run.cancelled.contains(&dep)
                || run.spawned.contains(&dep)
                || pre.contains(&dep)
            {
                continue;
            }
            let Some(r) = run.remaining.get_mut(&dep) else {
                continue;
            };
            *r = r.saturating_sub(1);
            // Release the borrow of `run.remaining` before spawning, which
            // needs `run` mutably again.
            let ready = *r == 0;
            if ready {
                self.spawn_and_track(run, semaphore, &dep);
            }
        }
    }

    /// A beam failed or was cancelled: emit a single Cancelled for every
    /// not-yet-spawned beam in its downstream closure. Those beams can never
    /// reach an in-degree of zero, so they would otherwise stay Pending.
    async fn cancel_dependents(
        &self,
        run: &mut RunLoop,
        graph: &BeamGraph,
        name: &str,
        nodes: &HashSet<String>,
    ) {
        for dep in graph.transitive_dependents(name) {
            if nodes.contains(&dep)
                && !run.spawned.contains(&dep)
                && run.cancelled.insert(dep.clone())
            {
                let _ = self
                    .tx
                    .send(SchedulerEvent::BeamCompleted {
                        name: dep,
                        status: BeamStatus::Cancelled,
                    })
                    .await;
            }
        }
    }

    /// Selects the executor for a beam from its `run.executor` name. A beam
    /// that declares none defaults to `local`, which the composition root
    /// always registers (its absence is a programming error). A beam that
    /// names an executor which is not registered is a configuration error:
    /// it is reported as `Err(name)` so the beam fails loudly, rather than
    /// being silently downgraded to running its commands on the host `local`
    /// executor. That downgrade would defeat the point of an untrusted beam
    /// asking to run sandboxed (for example in Docker).
    fn resolve_executor(&self, beam: &Beam) -> std::result::Result<Arc<dyn Executor>, String> {
        match beam
            .run
            .as_ref()
            .and_then(|r| r.executor.as_ref())
            .map(|e| e.name.as_str())
        {
            Some(name) => self
                .executors
                .get(name)
                .cloned()
                .ok_or_else(|| name.to_string()),
            None => Ok(self
                .executors
                .get("local")
                .cloned()
                .expect("no local executor registered")),
        }
    }

    /// Spawns a beam task and returns its cancellation sender together with the
    /// spawned task's id (used to attribute a panicked task to its beam).
    fn spawn_beam(
        &self,
        set: &mut JoinSet<(String, BeamOutcome)>,
        semaphore: &Option<Arc<Semaphore>>,
        beam_name: &str,
    ) -> (oneshot::Sender<()>, tokio::task::Id) {
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

        let beam = self.beams[beam_name].clone();
        let executor = self.resolve_executor(&beam);
        let task_env = TaskEnv {
            env: self.env.clone(),
            declared_env: self.declared_env.clone(),
            tx: self.tx.clone(),
            sem: semaphore.clone(),
            cache: self.cache.clone(),
            cache_enabled: self.cache_enabled,
            working_dir: self.working_dir.clone(),
        };

        let handle = set.spawn(run_beam_task(beam, executor, cancel_rx, task_env));
        (cancel_tx, handle.id())
    }
}

/// Mutable bookkeeping for one in-flight run: per-beam in-degree tracking, the
/// sets of already-spawned and already-cancelled beams, the live tasks and
/// their cancellation handles. Grouping this state lets the scheduling steps
/// be methods (`spawn_and_track`, `unblock_dependents`, `cancel_dependents`)
/// instead of an event loop juggling six parallel maps inline.
struct RunLoop {
    /// Remaining unmet dependencies per beam; a beam spawns when this hits 0.
    remaining: HashMap<String, usize>,
    /// Beams already emitted as Cancelled, so each is cancelled at most once.
    cancelled: HashSet<String>,
    /// Beams already spawned, so none is spawned twice.
    spawned: HashSet<String>,
    /// One cancellation sender per spawned beam: sending it makes the beam
    /// task's `select!` win and triggers cancellation.
    cancels: HashMap<String, oneshot::Sender<()>>,
    /// Maps each running task's id to its beam name, so a panicked task (whose
    /// return value is lost) can still be attributed to its beam.
    task_names: HashMap<tokio::task::Id, String>,
    /// The live beam tasks.
    set: JoinSet<(String, BeamOutcome)>,
    /// Set once the run is tearing down, so no further beam is spawned and the
    /// shutdown branch is not re-armed.
    shutting_down: bool,
}

/// Awaits the run's shutdown signal, or never resolves when none is armed.
/// `select!` polls every enabled branch, so the no-shutdown case needs a future
/// that stays pending rather than one that completes immediately.
async fn wait_for_shutdown(shutdown: &mut Option<oneshot::Receiver<()>>) {
    match shutdown {
        Some(rx) => {
            // A dropped sender means the run can never be shut down: treat it
            // like the absent case rather than firing a spurious teardown.
            if rx.await.is_err() {
                std::future::pending::<()>().await
            }
        }
        None => std::future::pending::<()>().await,
    }
}

impl RunLoop {
    fn new(remaining: HashMap<String, usize>) -> Self {
        Self {
            remaining,
            cancelled: HashSet::new(),
            spawned: HashSet::new(),
            cancels: HashMap::new(),
            task_names: HashMap::new(),
            set: JoinSet::new(),
            shutting_down: false,
        }
    }
}

/// The shared scheduler state a beam task needs, cloned once per spawn. Grouped
/// so [`run_beam_task`] takes a handful of arguments instead of a long list.
struct TaskEnv {
    env: HashMap<String, String>,
    declared_env: BTreeMap<String, String>,
    tx: mpsc::Sender<SchedulerEvent>,
    sem: Option<Arc<Semaphore>>,
    cache: Arc<BeamCache>,
    cache_enabled: bool,
    working_dir: PathBuf,
}

/// Runs a single beam to completion: acquires the parallelism permit, applies
/// gating and cache, then executes while racing cancellation. Emits the beam's
/// lifecycle events and returns its scheduling outcome.
async fn run_beam_task(
    beam: Beam,
    executor: std::result::Result<Arc<dyn Executor>, String>,
    mut cancel_rx: oneshot::Receiver<()>,
    task_env: TaskEnv,
) -> (String, BeamOutcome) {
    let TaskEnv {
        env,
        declared_env,
        tx,
        sem,
        cache,
        cache_enabled,
        working_dir,
    } = task_env;

    let _permit = match sem {
        // The semaphore is owned by the scheduler and never closed while the
        // run is in flight, so acquisition cannot fail here. Racing the wait
        // against cancellation lets a beam queued for a parallelism slot be
        // cancelled before it ever starts, instead of only once it runs.
        Some(s) => {
            tokio::select! {
                permit = s.acquire_owned() => {
                    Some(permit.expect("run semaphore is never closed during a run"))
                }
                _ = &mut cancel_rx => {
                    let _ = tx
                        .send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status: BeamStatus::Cancelled,
                        })
                        .await;
                    let outcome = if beam.allow_failure {
                        BeamOutcome::Ok
                    } else {
                        BeamOutcome::Cancelled
                    };
                    return (beam.name, outcome);
                }
            }
        }
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

    // `dir` rebases everything the beam does (gates, inputs/outputs, run
    // commands) onto that directory. A relative `dir` joins onto the Beamfile
    // directory; an absolute one replaces it (Path::join semantics). This
    // shadows the binding from `TaskEnv`, so `gate_skip_reason`,
    // `cache_lookup_blocking` and the `ExecutionInput` below all inherit it.
    let working_dir = match &beam.dir {
        Some(dir) => working_dir.join(dir),
        None => working_dir,
    };

    // A declared `dir` that is not an existing directory is a configuration
    // error: fail loudly with the offending path instead of leaking a raw
    // `sh: cannot cd` or a confusing cache miss. Mirrors the unknown-executor
    // failure path below.
    if beam.dir.is_some() && !working_dir.is_dir() {
        let err = anyhow::anyhow!(
            "working directory does not exist: {}",
            working_dir.display()
        );
        let _ = tx
            .send(SchedulerEvent::BeamOutput {
                name: beam.name.clone(),
                line: format!("aurora: {err:#}"),
                is_stderr: true,
            })
            .await;
        let (status, outcome) = classify_execution(&Err(err), beam.allow_failure, Duration::ZERO);
        let _ = tx
            .send(SchedulerEvent::BeamCompleted {
                name: beam.name.clone(),
                status,
            })
            .await;
        return (beam.name, outcome);
    }

    // A beam that named an executor which is not registered is a
    // configuration error: fail it loudly instead of silently downgrading
    // to the host `local` executor. Checked before gating and cache so a
    // typo cannot hide behind a skip or a cache hit.
    let executor = match executor {
        Ok(executor) => executor,
        Err(name) => {
            let _ = tx
                .send(SchedulerEvent::BeamOutput {
                    name: beam.name.clone(),
                    line: format!("aurora: unknown executor '{name}'"),
                    is_stderr: true,
                })
                .await;
            let (status, outcome) = classify_execution(
                &Err(anyhow::anyhow!("unknown executor '{name}'")),
                beam.allow_failure,
                Duration::ZERO,
            );
            let _ = tx
                .send(SchedulerEvent::BeamCompleted {
                    name: beam.name.clone(),
                    status,
                })
                .await;
            return (beam.name, outcome);
        }
    };

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

    // The cache key answers "would running this beam produce the same result?",
    // not merely "did its input files change?". So it covers the beam's
    // definition as well: its resolved commands (which already carry the
    // variables, the `--var` overrides and the positional arguments), the
    // executor and its settings, the working directory, and the declared
    // environment. Hashing the inputs alone would serve the previous run's
    // result after an edit to any of these.
    let run = beam.run.as_ref();
    let executor_config = run.and_then(|r| r.executor.as_ref());
    let definition_hash = BeamDefinition {
        commands: run.map(|r| r.commands.as_slice()).unwrap_or(&[]),
        executor: executor_config.map(|e| e.name.as_str()),
        executor_config: executor_config.map(|e| &e.config),
        dir: beam.dir.as_deref(),
        env: Some(&declared_env),
    }
    .hash();

    // Cache: on a valid hit, replay the recorded output and skip. The lookup
    // hashes the inputs (reading whole files) and stats the outputs, so it runs
    // on a blocking thread rather than stalling the async runtime.
    let inputs_hash = if cache_enabled && !beam.inputs.is_empty() {
        match cache_lookup_blocking(
            &cache,
            &beam.name,
            &beam.inputs,
            &beam.outputs,
            &definition_hash,
            &working_dir,
        )
        .await
        {
            CacheLookup::Hit { stdout, stderr } => {
                replay_cached_lines(&tx, &beam.name, stdout, stderr).await;
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
            CacheLookup::Miss { hash } => hash,
        }
    } else {
        None
    };

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
            if let Some(hash) = inputs_hash {
                // Persist off the async runtime: writing the entry serializes
                // and writes the whole captured output to disk.
                save_cache_blocking(&cache, &beam.name, hash, stdout_lines, stderr_lines).await;
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
}

/// Runs a gating shell command and reports whether it succeeded (exit 0). A
/// command that fails to launch counts as "not succeeded".
///
/// By design this always runs on the local host via `sh -c`, regardless of the
/// beam's `run.executor`: `skip_if` and `condition` are evaluated locally even
/// when the beam itself runs in Docker or a WASM plugin. They are meant to
/// decide *whether* to run the beam, using the host's state.
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
/// Outcome of a cache probe: a hit carries the recorded output to replay; a
/// miss carries the inputs hash (if any) to persist once the beam has run.
enum CacheLookup {
    Hit {
        stdout: Vec<String>,
        stderr: Vec<String>,
    },
    Miss {
        hash: Option<String>,
    },
}

/// Probes the cache on a blocking thread: hashes the inputs (reading whole
/// files), and on a hash and output match loads the recorded logs. The cache
/// is filesystem-backed and synchronous by design, so it must not run on the
/// async runtime where it would stall other beams and delay cancellation.
async fn cache_lookup_blocking(
    cache: &Arc<BeamCache>,
    beam_name: &str,
    inputs: &[String],
    outputs: &[String],
    definition_hash: &str,
    working_dir: &Path,
) -> CacheLookup {
    let cache = cache.clone();
    let beam_name = beam_name.to_string();
    let inputs = inputs.to_vec();
    let outputs = outputs.to_vec();
    let definition_hash = definition_hash.to_string();
    let working_dir = working_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        // `hash_inputs_at` yields `None` when no input file matches: the beam
        // cannot be keyed and must run. The definition alone must never key an
        // entry, or a beam whose inputs vanished would stay cached forever.
        let hash = cache
            .hash_inputs_at(&working_dir, &inputs)
            .ok()
            .flatten()
            .map(|h| BeamCache::key(&h, &definition_hash));
        if let Some(ref hash) = hash {
            if cache.is_valid(&beam_name, hash, &outputs, &working_dir) {
                let (stdout, stderr) = cache.load_logs(&beam_name);
                return CacheLookup::Hit { stdout, stderr };
            }
        }
        CacheLookup::Miss { hash }
    })
    .await
    .unwrap_or(CacheLookup::Miss { hash: None })
}

/// Persists a beam's cache entry on a blocking thread. Failures are ignored:
/// a cache that cannot be written must never fail the run.
async fn save_cache_blocking(
    cache: &Arc<BeamCache>,
    beam_name: &str,
    hash: String,
    stdout: Vec<String>,
    stderr: Vec<String>,
) {
    let cache = cache.clone();
    let beam_name = beam_name.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        cache.save_with_logs(&beam_name, &hash, &stdout, &stderr)
    })
    .await;
}

/// Replays cached output lines as `BeamOutput` events, stdout then stderr.
async fn replay_cached_lines(
    tx: &mpsc::Sender<SchedulerEvent>,
    beam_name: &str,
    stdout: Vec<String>,
    stderr: Vec<String>,
) {
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
