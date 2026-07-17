//! Watch mode: re-run a target and its dependents when their declared inputs
//! (or the Beamfile) change.
//!
//! The pieces here are deliberately split so the logic is testable without
//! `notify` or the filesystem: [`glob_root`] and `build_watch_set` compute what
//! to watch, [`classify_path`] filters raw events, [`debounce_loop`] coalesces
//! them, and only [`Watcher`] wires those onto real `notify` events.

use aurora_core::ast::Beam;
use aurora_core::dag::BeamGraph;
use aurora_core::events::WatchTrigger;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// Quiet period after the last relevant change before a [`WatchTrigger`] is
/// emitted. Matches the debounce cargo-watch/watchexec use: long enough to
/// coalesce an editor's multi-file save, short enough to feel immediate.
pub const DEBOUNCE: Duration = Duration::from_millis(250);

/// The fixed directory prefix of a glob pattern: the longest leading run of
/// components that contain no glob metacharacter. `src/**/*.rs` yields `src`,
/// `*.rs` yields an empty path (the pattern's base directory), and a literal
/// `a/b/c.txt` yields itself. Watching this root recursively (rather than the
/// files currently matched) is what lets a newly created file that matches the
/// glob still trigger a re-run.
pub fn glob_root(pattern: &str) -> PathBuf {
    let mut root = PathBuf::new();
    for component in Path::new(pattern).components() {
        let part = component.as_os_str().to_string_lossy();
        if part.contains(['*', '?', '[']) {
            break;
        }
        root.push(component);
    }
    root
}

/// What to watch for a given target: the directory roots to register
/// recursively with `notify`, the absolute glob patterns used to keep only
/// relevant events, the Beamfile path (always watched), and whether any beam in
/// the closure declared usable inputs. When `has_inputs` is false the caller
/// warns and watches the Beamfile alone.
pub struct WatchSet {
    pub roots: Vec<PathBuf>,
    pub patterns: Vec<glob::Pattern>,
    pub beamfile: PathBuf,
    pub has_inputs: bool,
}

/// The set of beam names in `target`'s transitive closure (the target plus all
/// its transitive dependencies). Falls back to every beam if the graph cannot
/// be built (a cycle): the scheduler will surface that error itself, and an
/// over-broad watch set is harmless.
pub fn closure_of(beams: &[Beam], target: &str) -> HashSet<String> {
    let deps: Vec<(String, Vec<String>)> = beams
        .iter()
        .map(|b| (b.name.clone(), b.depends_on.clone()))
        .collect();
    match BeamGraph::from_deps(deps) {
        Ok(graph) => graph.transitive_deps(target).into_iter().collect(),
        Err(_) => beams.iter().map(|b| b.name.clone()).collect(),
    }
}

/// True when a pattern would resolve outside the Beamfile directory: an absolute
/// path (which `join` lets replace the base) or one with a `..` component. Such
/// a pattern is skipped, mirroring the cache's `escapes_base_dir` guard: a
/// Beamfile is untrusted and must not make the watcher register roots outside
/// its own tree.
fn escapes_base_dir(pattern: &str) -> bool {
    let candidate = Path::new(pattern);
    candidate.is_absolute()
        || candidate
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// The nearest existing ancestor directory of `path` (including `path` itself).
/// Returns `None` when no ancestor exists on disk. Used so a literal-file root
/// (or a glob root whose leaf is not yet created) still registers on its parent
/// directory, which catches the file being created or atomically replaced.
fn nearest_existing_dir(path: &Path) -> Option<PathBuf> {
    let mut current = path;
    loop {
        if current.is_dir() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

/// Computes the [`WatchSet`] for `target`'s closure. Roots are deduplicated; a
/// pattern whose root has no existing ancestor directory contributes no root but
/// still contributes its filter pattern, so it starts matching once the tree
/// appears (after a Beamfile change recomputes the set).
pub fn build_watch_set(
    beams: &[Beam],
    closure: &HashSet<String>,
    working_dir: &Path,
    beamfile: &Path,
) -> WatchSet {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut patterns: Vec<glob::Pattern> = Vec::new();

    for b in beams.iter().filter(|b| closure.contains(&b.name)) {
        let effective_dir = match &b.dir {
            Some(dir) => working_dir.join(dir),
            None => working_dir.to_path_buf(),
        };
        for pattern in &b.inputs {
            if escapes_base_dir(pattern) {
                continue;
            }
            let absolute = effective_dir.join(pattern);
            if let Ok(compiled) = glob::Pattern::new(&absolute.to_string_lossy()) {
                patterns.push(compiled);
            }
            if let Some(dir) = nearest_existing_dir(&effective_dir.join(glob_root(pattern))) {
                if !roots.contains(&dir) {
                    roots.push(dir);
                }
            }
        }
    }

    WatchSet {
        has_inputs: !patterns.is_empty(),
        roots,
        patterns,
        beamfile: beamfile.to_path_buf(),
    }
}

/// Classifies a raw `notify` path against the watch set. Returns `Some(true)`
/// when it is the Beamfile, `Some(false)` when it matches an input glob, and
/// `None` otherwise. Paths under `.aurora/` (the cache) never match: a beam's
/// own cache write must not re-trigger the watch.
pub fn classify_path(path: &Path, set: &WatchSet) -> Option<bool> {
    if path.components().any(|c| c.as_os_str() == ".aurora") {
        return None;
    }
    if path == set.beamfile {
        return Some(true);
    }
    if set.patterns.iter().any(|p| p.matches_path(path)) {
        return Some(false);
    }
    None
}

/// Coalesces relevant change signals into one [`WatchTrigger`] per quiet
/// period. Each item on `raw_rx` is a relevant change (`true` when it was the
/// Beamfile). After the first signal, the loop keeps resetting a `quiet` timer
/// on every further signal; when the timer finally fires it emits a single
/// trigger whose `beamfile_changed` is the OR over the whole burst, then waits
/// for the next burst. Returns when `raw_rx` closes (the [`Watcher`] dropped),
/// flushing a pending burst first so no change is lost on teardown.
pub async fn debounce_loop(
    mut raw_rx: mpsc::UnboundedReceiver<bool>,
    trigger_tx: mpsc::Sender<WatchTrigger>,
    quiet: Duration,
) {
    loop {
        // Block until the first signal of a new burst; channel closed => stop.
        let Some(first) = raw_rx.recv().await else {
            return;
        };
        let mut beamfile_changed = first;

        loop {
            tokio::select! {
                signal = raw_rx.recv() => match signal {
                    Some(is_beamfile) => beamfile_changed |= is_beamfile,
                    None => {
                        // Closed mid-burst: flush what we have, then stop.
                        let _ = trigger_tx
                            .send(WatchTrigger { beamfile_changed })
                            .await;
                        return;
                    }
                },
                _ = tokio::time::sleep(quiet) => {
                    let _ = trigger_tx
                        .send(WatchTrigger { beamfile_changed })
                        .await;
                    break;
                }
            }
        }
    }
}

/// Static detection of an output-to-input loop: a beam whose `outputs` match an
/// `inputs` glob of the closure would re-trigger the watch after every run.
/// Returns one warning per offending output. This is a heuristic (patterns are
/// matched relative to the Beamfile directory, ignoring per-beam `dir`), so it
/// only warns; the overlap can be intentional and the cache usually stabilizes
/// the loop on the second cycle.
pub fn detect_output_input_overlap(beams: &[Beam], closure: &HashSet<String>) -> Vec<String> {
    let patterns: Vec<glob::Pattern> = beams
        .iter()
        .filter(|b| closure.contains(&b.name))
        .flat_map(|b| b.inputs.iter())
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();

    let mut warnings = Vec::new();
    for b in beams.iter().filter(|b| closure.contains(&b.name)) {
        for output in &b.outputs {
            if patterns.iter().any(|p| p.matches(output)) {
                warnings.push(format!(
                    "beam '{}' output '{}' matches a watched input; the watch may re-trigger itself",
                    b.name, output
                ));
            }
        }
    }
    warnings
}
