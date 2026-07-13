//! Internal library of the `aurora` binary: exposes the components
//! that are testable independently of the TUI (headless mode).

pub mod headless;
pub mod plugins;

use aurora_core::ast::Beam;
use aurora_core::events::SchedulerEvent;
use aurora_core::scheduler::Scheduler;
use aurora_executor_api::Executor;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Resolves the effective parallelism cap for a run. An explicit value from
/// the Beamfile's `aurora { parallelism = N }` block is honored as-is; an
/// absent value defaults to the host's available parallelism rather than
/// running unbounded, so a Beamfile with many independent beams cannot spawn
/// one process per beam at once (an accidental fork bomb). The scheduler
/// treats `None` as unbounded, so this always returns `Some`.
pub fn resolve_max_parallelism(configured: Option<usize>) -> Option<usize> {
    Some(configured.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }))
}

/// Builds a [`Scheduler`] from the shared run parameters, applying the cache
/// setting and the default parallelism cap in one place. Centralizes the
/// wiring shared by the initial run and the TUI rerun path so it cannot drift
/// between the two.
#[allow(clippy::too_many_arguments)]
pub fn build_scheduler(
    beams: Vec<Beam>,
    executors: HashMap<String, Arc<dyn Executor>>,
    tx: mpsc::Sender<SchedulerEvent>,
    max_parallelism: Option<usize>,
    working_dir: PathBuf,
    env: HashMap<String, String>,
    declared_env: BTreeMap<String, String>,
    cache_enabled: bool,
) -> Scheduler {
    let max_parallelism = resolve_max_parallelism(max_parallelism);
    let scheduler = Scheduler::new(beams, executors, tx, max_parallelism, working_dir, env)
        .with_declared_env(declared_env);
    if cache_enabled {
        scheduler
    } else {
        scheduler.without_cache()
    }
}
