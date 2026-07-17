//! Watch mode: re-run a target and its dependents when their declared inputs
//! (or the Beamfile) change.
//!
//! The pieces here are deliberately split so the logic is testable without
//! `notify` or the filesystem: [`glob_root`] and `build_watch_set` compute what
//! to watch, [`classify_path`] filters raw events, [`debounce_loop`] coalesces
//! them, and only [`Watcher`] wires those onto real `notify` events.

use std::path::{Path, PathBuf};
use std::time::Duration;

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
