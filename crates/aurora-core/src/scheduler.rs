use crate::ast::Beam;
use crate::cache::BeamCache;
use crate::dag::BeamGraph;
use aurora_executor_api::{Executor, ExecutionInput};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub enum BeamStatus {
    Pending,
    Running,
    Success { duration: Duration, cached: bool },
    Skipped { reason: SkipReason },
    Failed { exit_code: i32, duration: Duration },
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    Cached,
    ConditionFalse,
}

#[derive(Debug)]
pub enum SchedulerEvent {
    BeamStarted { name: String },
    BeamCompleted { name: String, status: BeamStatus },
    BeamOutput { name: String, line: String, is_stderr: bool },
    AllDone { success: bool },
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

    pub async fn run(self, root: &str, pre_success: &[String]) -> Result<bool> {
        let deps: Vec<(String, Vec<String>)> = self.beams.values()
            .map(|b| (b.name.clone(), b.depends_on.clone()))
            .collect();
        let graph = BeamGraph::from_deps(deps)?;
        let levels = graph.execution_levels(root)?;

        let semaphore = self.max_parallelism.map(|n| Arc::new(Semaphore::new(n)));
        let mut overall_success = true;
        let mut cancelled: Vec<String> = vec![];

        for level in &levels {
            let mut set = JoinSet::new();
            for beam_name in level {
                // Beam déjà réussi — silencieux, dépendants débloqués normalement
                if pre_success.contains(beam_name) {
                    continue;
                }
                if cancelled.contains(beam_name) {
                    let _ = self.tx.send(SchedulerEvent::BeamCompleted {
                        name: beam_name.clone(),
                        status: BeamStatus::Cancelled,
                    }).await;
                    continue;
                }
                let beam = self.beams[beam_name].clone();
                let env = self.env.clone();
                let executor_name = beam.run.as_ref()
                    .and_then(|r| r.executor.as_ref())
                    .map(|e| e.name.as_str())
                    .unwrap_or("local")
                    .to_string();
                let executor = self.executors
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
                    } else { None };

                    let _ = tx.send(SchedulerEvent::BeamStarted { name: beam.name.clone() }).await;

                    if beam.run.is_none() {
                        let _ = tx.send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status: BeamStatus::Success { duration: Duration::ZERO, cached: false },
                        }).await;
                        return (beam.name, true);
                    }

                    if let Some(cond) = &beam.skip_if {
                        let skip = tokio::process::Command::new("sh")
                            .arg("-c").arg(cond)
                            .envs(&env)
                            .status().await
                            .map(|s| s.success()).unwrap_or(false);
                        if skip {
                            let _ = tx.send(SchedulerEvent::BeamCompleted {
                                name: beam.name.clone(),
                                status: BeamStatus::Skipped { reason: SkipReason::ConditionFalse },
                            }).await;
                            return (beam.name, true);
                        }
                    }

                    // Cache check — uniquement si inputs/outputs définis
                    let inputs_hash = if !beam.inputs.is_empty() {
                        cache.hash_inputs_at(&working_dir, &beam.inputs).ok()
                    } else {
                        None
                    };

                    if let Some(ref hash) = inputs_hash {
                        if cache.is_valid(&beam.name, hash, &beam.outputs) {
                            // Rejouer les logs de la dernière exécution
                            let (stdout, stderr) = cache.load_logs(&beam.name);
                            for line in stdout {
                                let _ = tx.send(SchedulerEvent::BeamOutput {
                                    name: beam.name.clone(),
                                    line,
                                    is_stderr: false,
                                }).await;
                            }
                            for line in stderr {
                                let _ = tx.send(SchedulerEvent::BeamOutput {
                                    name: beam.name.clone(),
                                    line,
                                    is_stderr: true,
                                }).await;
                            }
                            let _ = tx.send(SchedulerEvent::BeamCompleted {
                                name: beam.name.clone(),
                                status: BeamStatus::Skipped { reason: SkipReason::Cached },
                            }).await;
                            return (beam.name, true);
                        }
                    }

                    let run = beam.run.as_ref().unwrap();

                    // Config executor (ex: image docker)
                    let executor_config = run.executor.as_ref()
                        .map(|e| {
                            let map: serde_json::Map<String, serde_json::Value> = e.config.iter()
                                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                                .collect();
                            serde_json::Value::Object(map)
                        })
                        .unwrap_or(serde_json::json!({}));

                    // Canal pour streamer les lignes de sortie en temps réel
                    let (out_tx, mut out_rx) = mpsc::channel::<(String, bool)>(256);
                    let tx_fwd = tx.clone();
                    let beam_name_fwd = beam.name.clone();
                    let fwd_handle = tokio::spawn(async move {
                        let mut stdout_lines: Vec<String> = vec![];
                        let mut stderr_lines: Vec<String> = vec![];
                        while let Some((line, is_stderr)) = out_rx.recv().await {
                            let _ = tx_fwd.send(SchedulerEvent::BeamOutput {
                                name: beam_name_fwd.clone(),
                                line: line.clone(),
                                is_stderr,
                            }).await;
                            if is_stderr { stderr_lines.push(line); } else { stdout_lines.push(line); }
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
                    let result = executor.execute(input).await;
                    // Attend que toutes les lignes aient été forwardées
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
                                BeamStatus::Success { duration, cached: false }
                            } else {
                                BeamStatus::Failed { exit_code: output.exit_code, duration }
                            };
                            let _ = tx.send(SchedulerEvent::BeamCompleted {
                                name: beam.name.clone(),
                                status,
                            }).await;
                            (beam.name, success)
                        }
                        Err(_) => {
                            let _ = tx.send(SchedulerEvent::BeamCompleted {
                                name: beam.name.clone(),
                                status: BeamStatus::Failed { exit_code: -1, duration: start.elapsed() },
                            }).await;
                            (beam.name, false)
                        }
                    }
                });
            }

            while let Some(result) = set.join_next().await {
                if let Ok((name, success)) = result {
                    if !success {
                        overall_success = false;
                        let dependents = graph.direct_dependents(&name);
                        cancelled.extend(dependents);
                    }
                }
            }
        }

        let _ = self.tx.send(SchedulerEvent::AllDone { success: overall_success }).await;
        Ok(overall_success)
    }
}
