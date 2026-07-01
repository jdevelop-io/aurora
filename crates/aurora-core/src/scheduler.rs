use crate::ast::{Beam, ConditionClause, ConditionOp};
use crate::cache::BeamCache;
use crate::dag::BeamGraph;
use anyhow::Result;
use aurora_executor_api::{ExecutionInput, Executor};
use std::collections::HashMap;
use std::path::PathBuf;
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
            working_dir,
            env,
        }
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
        use std::collections::HashSet;

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
            let deg = graph
                .direct_dependencies(n)
                .into_iter()
                .filter(|d| nodes.contains(d) && !pre.contains(d))
                .count();
            remaining.insert(n.clone(), deg);
        }

        let mut cancelled: HashSet<String> = HashSet::new();
        let mut spawned: HashSet<String> = HashSet::new();
        let mut set: JoinSet<(String, BeamOutcome)> = JoinSet::new();
        // One cancellation sender per spawned beam: receiving it inside the
        // task makes the `select!` win and triggers the cancellation.
        let mut cancels: HashMap<String, oneshot::Sender<()>> = HashMap::new();

        for n in &nodes {
            if pre.contains(n) {
                continue;
            }
            if remaining[n] == 0 {
                let s = self.spawn_beam(&mut set, &semaphore, n);
                cancels.insert(n.clone(), s);
                spawned.insert(n.clone());
            }
        }

        loop {
            tokio::select! {
                joined = set.join_next() => {
                    let Some(result) = joined else { break };
                    let (name, outcome) = match result {
                        Ok(pair) => pair,
                        Err(_) => {
                            // Panicked task: its dependents cannot be unblocked;
                            // the loop ends once the set is empty.
                            overall_success = false;
                            continue;
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
                                        let s = self.spawn_beam(&mut set, &semaphore, &dep);
                                        cancels.insert(dep.clone(), s);
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

    fn spawn_beam(
        &self,
        set: &mut JoinSet<(String, BeamOutcome)>,
        semaphore: &Option<Arc<Semaphore>>,
        beam_name: &str,
    ) -> oneshot::Sender<()> {
        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

        let beam = self.beams[beam_name].clone();
        let env = self.env.clone();
        let executor_name = beam
            .run
            .as_ref()
            .and_then(|r| r.executor.as_ref())
            .map(|e| e.name.as_str())
            .unwrap_or("local")
            .to_string();
        let executor = self
            .executors
            .get(&executor_name)
            .or_else(|| self.executors.get("local"))
            .cloned()
            .expect("no local executor registered");
        let tx = self.tx.clone();
        let sem = semaphore.clone();
        let cache = self.cache.clone();
        let working_dir = self.working_dir.clone();

        set.spawn(async move {
            let _permit = if let Some(s) = sem {
                Some(s.acquire_owned().await.unwrap())
            } else {
                None
            };

            let _ = tx
                .send(SchedulerEvent::BeamStarted {
                    name: beam.name.clone(),
                })
                .await;

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

            if let Some(cond) = &beam.skip_if {
                let skip = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(cond)
                    .current_dir(&working_dir)
                    .env_clear()
                    .envs(&env)
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false);
                if skip {
                    let _ = tx
                        .send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status: BeamStatus::Skipped {
                                reason: SkipReason::SkipIf,
                            },
                        })
                        .await;
                    return (beam.name, BeamOutcome::Ok);
                }
            }

            // A `condition { }` block gates execution: the beam runs only when
            // the condition holds (all/any of the shell clauses succeed).
            if let Some(condition) = &beam.condition {
                let mut clause_results = Vec::with_capacity(condition.clauses.len());
                for ConditionClause::Shell(cmd) in &condition.clauses {
                    let ok = tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(cmd)
                        .current_dir(&working_dir)
                        .env_clear()
                        .envs(&env)
                        .status()
                        .await
                        .map(|s| s.success())
                        .unwrap_or(false);
                    clause_results.push(ok);
                }
                let met = match condition.op {
                    ConditionOp::All => clause_results.iter().all(|&ok| ok),
                    ConditionOp::Any => clause_results.iter().any(|&ok| ok),
                };
                if !met {
                    let _ = tx
                        .send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status: BeamStatus::Skipped {
                                reason: SkipReason::ConditionNotMet,
                            },
                        })
                        .await;
                    return (beam.name, BeamOutcome::Ok);
                }
            }

            let inputs_hash = if !beam.inputs.is_empty() {
                cache.hash_inputs_at(&working_dir, &beam.inputs).ok()
            } else {
                None
            };

            if let Some(ref hash) = inputs_hash {
                if cache.is_valid(&beam.name, hash, &beam.outputs) {
                    let (stdout, stderr) = cache.load_logs(&beam.name);
                    for line in stdout {
                        let _ = tx
                            .send(SchedulerEvent::BeamOutput {
                                name: beam.name.clone(),
                                line,
                                is_stderr: false,
                            })
                            .await;
                    }
                    for line in stderr {
                        let _ = tx
                            .send(SchedulerEvent::BeamOutput {
                                name: beam.name.clone(),
                                line,
                                is_stderr: true,
                            })
                            .await;
                    }
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

            let run = beam.run.as_ref().unwrap();

            let executor_config = run
                .executor
                .as_ref()
                .map(|e| {
                    let map: serde_json::Map<String, serde_json::Value> = e
                        .config
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                        .collect();
                    serde_json::Value::Object(map)
                })
                .unwrap_or(serde_json::json!({}));

            let (out_tx, mut out_rx) = mpsc::channel::<(String, bool)>(256);
            let tx_fwd = tx.clone();
            let beam_name_fwd = beam.name.clone();
            let fwd_handle = tokio::spawn(async move {
                let mut stdout_lines: Vec<String> = vec![];
                let mut stderr_lines: Vec<String> = vec![];
                while let Some((line, is_stderr)) = out_rx.recv().await {
                    let _ = tx_fwd
                        .send(SchedulerEvent::BeamOutput {
                            name: beam_name_fwd.clone(),
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

            let input = ExecutionInput {
                commands: run.commands.clone(),
                env,
                working_dir: working_dir.clone(),
                config: executor_config,
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

            match result {
                Ok(output) => {
                    let duration = start.elapsed();
                    let success = output.success();
                    if success {
                        if let Some(ref hash) = inputs_hash {
                            let _ = cache.save_with_logs(
                                &beam.name,
                                hash,
                                &stdout_lines,
                                &stderr_lines,
                            );
                        }
                    }
                    let status = if success {
                        BeamStatus::Success {
                            duration,
                            cached: false,
                        }
                    } else if beam.allow_failure {
                        BeamStatus::FailedAllowed {
                            exit_code: output.exit_code,
                            duration,
                        }
                    } else {
                        BeamStatus::Failed {
                            exit_code: output.exit_code,
                            duration,
                        }
                    };
                    let _ = tx
                        .send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status,
                        })
                        .await;
                    let counts = success || beam.allow_failure;
                    (
                        beam.name,
                        if counts {
                            BeamOutcome::Ok
                        } else {
                            BeamOutcome::Failed
                        },
                    )
                }
                Err(e) => {
                    let duration = start.elapsed();
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
                    let status = if beam.allow_failure {
                        BeamStatus::FailedAllowed {
                            exit_code: -1,
                            duration,
                        }
                    } else {
                        BeamStatus::Failed {
                            exit_code: -1,
                            duration,
                        }
                    };
                    let _ = tx
                        .send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status,
                        })
                        .await;
                    (
                        beam.name,
                        if beam.allow_failure {
                            BeamOutcome::Ok
                        } else {
                            BeamOutcome::Failed
                        },
                    )
                }
            }
        });

        cancel_tx
    }
}
