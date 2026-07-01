//! Internal library of the `aurora` binary: exposes the components
//! that are testable independently of the TUI (headless mode).

pub mod headless;

use aurora_core::ast::Beam;
use aurora_core::scheduler::{Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Builds a [`Scheduler`] from the shared run parameters, applying the cache
/// setting in one place. Centralizes the wiring shared by the initial run and
/// the TUI rerun path so it cannot drift between the two.
#[allow(clippy::too_many_arguments)]
pub fn build_scheduler(
    beams: Vec<Beam>,
    executors: HashMap<String, Arc<dyn Executor>>,
    tx: mpsc::Sender<SchedulerEvent>,
    max_parallelism: Option<usize>,
    working_dir: PathBuf,
    env: HashMap<String, String>,
    cache_enabled: bool,
) -> Scheduler {
    let scheduler = Scheduler::new(beams, executors, tx, max_parallelism, working_dir, env);
    if cache_enabled {
        scheduler
    } else {
        scheduler.without_cache()
    }
}
