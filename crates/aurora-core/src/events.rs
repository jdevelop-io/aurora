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
    AllDone {
        success: bool,
    },
}
