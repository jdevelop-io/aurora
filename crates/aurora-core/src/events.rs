//! The presentation contract between the scheduler (which emits progress) and
//! its consumers (the TUI and the headless renderer).
//!
//! These types describe *what happened* to a beam, independently of the engine
//! that drives execution. Keeping them in their own module lets a consumer
//! depend on the event model without depending on the scheduler internals.

use std::time::Duration;

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
    /// A non-fatal advisory about a beam, surfaced to the user but never
    /// affecting the run's outcome. Emitted, for example, when a declared input
    /// pattern matches no file and so silently protects nothing in the cache.
    Warning {
        name: String,
        message: String,
    },
    AllDone {
        success: bool,
    },
}

/// Emitted by the watcher once a quiet period has elapsed after one or more
/// relevant filesystem changes. Coalesces a whole burst of events into a single
/// re-run request. `beamfile_changed` is the OR over the burst: it is true when
/// any change in the window touched the Beamfile, so the supervisor knows it
/// must re-parse before the next cycle rather than only re-running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchTrigger {
    pub beamfile_changed: bool,
}
