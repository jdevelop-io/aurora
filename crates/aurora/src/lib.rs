//! Internal library of the `aurora` binary: exposes the components
//! that are testable independently of the TUI (headless mode).

pub mod headless;
pub mod plugins;

use anyhow::{bail, Result};
use aurora_core::ast::{Beam, BeamFile};
use aurora_core::events::SchedulerEvent;
use aurora_core::scheduler::Scheduler;
use aurora_executor_api::Executor;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Similarity above which a candidate is offered as a "did you mean". Jaro-Winkler
/// rewards a shared prefix, which is what a typo usually preserves.
const SUGGESTION_THRESHOLD: f64 = 0.8;

/// The candidate closest to `input`, when one is close enough to be worth
/// suggesting. Returns `None` rather than a distant, misleading guess.
fn closest<'a>(input: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    candidates
        .map(|candidate| (candidate, strsim::jaro_winkler(input, candidate)))
        .filter(|(_, score)| *score >= SUGGESTION_THRESHOLD)
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(candidate, _)| candidate)
}

/// Resolves the beam to run: the explicitly invoked one, otherwise the
/// `aurora { default = ... }` beam. The resolved name is checked against the
/// declared beams, because a target that does not exist schedules nothing: left
/// unchecked it would exit 0, turning a typo in CI into a green build.
pub fn resolve_target(beam_file: &BeamFile, explicit: Option<&str>) -> Result<String> {
    let target = match explicit {
        Some(name) => name.to_string(),
        None => match beam_file.config.as_ref().and_then(|c| c.default.as_ref()) {
            Some(default) => default.clone(),
            None => bail!("No beam specified and no default configured in aurora {{ }}"),
        },
    };
    ensure_beam_exists(beam_file, &target)?;
    Ok(target)
}

/// Fails when `target` is not a declared beam, suggesting the closest name.
pub fn ensure_beam_exists(beam_file: &BeamFile, target: &str) -> Result<()> {
    if beam_file.beams.iter().any(|b| b.name == target) {
        return Ok(());
    }
    match closest(target, beam_file.beams.iter().map(|b| b.name.as_str())) {
        Some(suggestion) => bail!("Unknown beam '{target}'. Did you mean '{suggestion}'?"),
        None => bail!("Unknown beam '{target}'. Run `aurora --list` to see the available beams."),
    }
}

/// Applies `--var key=value` overrides to the Beamfile's global variables.
///
/// An unknown key is an error: silently dropping it means the run proceeds with
/// the default value the user believed they had overridden. Only global
/// `variable` blocks are overridable; a beam-local one is private to its beam.
pub fn apply_var_overrides<'a>(
    beam_file: &mut BeamFile,
    overrides: impl Iterator<Item = &'a String>,
) -> Result<()> {
    for raw in overrides {
        let (key, value) = raw
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("Invalid --var format, expected key=value"))?;

        if !beam_file.variables.iter().any(|v| v.name == key) {
            match closest(key, beam_file.variables.iter().map(|v| v.name.as_str())) {
                Some(suggestion) => bail!(
                    "Unknown variable '{key}' passed with --var. Did you mean '{suggestion}'?"
                ),
                None => bail!(
                    "Unknown variable '{key}' passed with --var. \
                     Only global `variable` blocks can be overridden."
                ),
            }
        }

        for variable in beam_file.variables.iter_mut().filter(|v| v.name == key) {
            variable.default = value.to_string();
        }
    }
    Ok(())
}

/// Resolves once the process is asked to terminate: Ctrl-C anywhere, and also
/// SIGTERM on Unix (what a CI runner or an orchestrator sends to stop a job).
///
/// A run that ignored these would be killed on the default disposition, leaving
/// its beams' process subtrees behind; the caller uses this to tear the run down
/// while its executors can still reap their children.
pub async fn wait_for_termination_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        // A failure to register the SIGTERM handler must not cost us Ctrl-C.
        match signal(SignalKind::terminate()) {
            Ok(mut sigterm) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = sigterm.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

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
