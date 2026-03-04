use crate::ast::Beam;
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
    executor: Arc<dyn Executor>,
    tx: mpsc::Sender<SchedulerEvent>,
    max_parallelism: Option<usize>,
}

impl Scheduler {
    pub fn new(
        beams: Vec<Beam>,
        executor: Arc<dyn Executor>,
        tx: mpsc::Sender<SchedulerEvent>,
        max_parallelism: Option<usize>,
    ) -> Self {
        Self {
            beams: beams.into_iter().map(|b| (b.name.clone(), b)).collect(),
            executor,
            tx,
            max_parallelism,
        }
    }

    pub async fn run(self, root: &str) -> Result<bool> {
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
                if cancelled.contains(beam_name) {
                    let _ = self.tx.send(SchedulerEvent::BeamCompleted {
                        name: beam_name.clone(),
                        status: BeamStatus::Cancelled,
                    }).await;
                    continue;
                }
                let beam = self.beams[beam_name].clone();
                let executor = self.executor.clone();
                let tx = self.tx.clone();
                let sem = semaphore.clone();

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

                    let run = beam.run.as_ref().unwrap();
                    let input = ExecutionInput {
                        commands: run.commands.clone(),
                        env: std::env::vars().collect(),
                        working_dir: PathBuf::from("."),
                        config: serde_json::json!({}),
                    };

                    let start = Instant::now();
                    let result = executor.execute(input).await;

                    match result {
                        Ok(output) => {
                            let duration = start.elapsed();
                            let success = output.success();
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
