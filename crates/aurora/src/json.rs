//! Machine-readable NDJSON renderer: one JSON object per line on stdout, in
//! real time. The wire types here are the public contract (versioned by
//! `schema`), deliberately separate from `aurora_core`'s `SchedulerEvent` so a
//! refactor of the engine cannot silently break a consumer.

use std::io::Write;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use aurora_core::events::{BeamStatus, SchedulerEvent, SkipReason};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::reporter::Reporter;
use crate::time::now_iso8601;

/// The schema version stamped on every emitted line.
const SCHEMA: u32 = 1;

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum WireEvent {
    RunStarted {
        target: String,
        beams: Vec<String>,
        at: String,
    },
    BeamStarted {
        beam: String,
        at: String,
    },
    BeamOutput {
        beam: String,
        stream: &'static str,
        line: String,
    },
    BeamCompleted {
        beam: String,
        #[serde(flatten)]
        status: WireStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u128>,
        at: String,
    },
    RunCompleted {
        success: bool,
        duration_ms: u128,
        at: String,
    },
    Error {
        kind: String,
        message: String,
    },
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum WireStatus {
    Success { cached: bool },
    Skipped { reason: &'static str },
    Failed { exit_code: i32 },
    FailedAllowed { exit_code: i32 },
    Cancelled,
}

/// Maps a scheduler status to its wire form plus the optional duration.
/// A cached or skipped beam has no run to time, so its duration is `None`.
fn map_status(status: BeamStatus) -> (WireStatus, Option<u128>) {
    match status {
        BeamStatus::Success { duration, cached } => {
            let duration_ms = if cached {
                None
            } else {
                Some(duration.as_millis())
            };
            (WireStatus::Success { cached }, duration_ms)
        }
        BeamStatus::Skipped { reason } => {
            let reason = match reason {
                SkipReason::Cached => "cached",
                SkipReason::SkipIf => "skip_if",
                SkipReason::ConditionNotMet => "condition_not_met",
            };
            (WireStatus::Skipped { reason }, None)
        }
        BeamStatus::Failed {
            exit_code,
            duration,
        } => (WireStatus::Failed { exit_code }, Some(duration.as_millis())),
        BeamStatus::FailedAllowed {
            exit_code,
            duration,
        } => (
            WireStatus::FailedAllowed { exit_code },
            Some(duration.as_millis()),
        ),
        BeamStatus::Cancelled => (WireStatus::Cancelled, None),
        // Pending/Running are internal TUI states, never carried by BeamCompleted.
        BeamStatus::Pending | BeamStatus::Running => (WireStatus::Cancelled, None),
    }
}

pub struct JsonReporter<'a, W: Write> {
    target: String,
    beams: Vec<String>,
    out: &'a mut W,
}

impl<'a, W: Write> JsonReporter<'a, W> {
    pub fn new(target: String, beams: Vec<String>, out: &'a mut W) -> Self {
        Self { target, beams, out }
    }

    /// Serializes one event as a single line and flushes immediately: unbuffered
    /// streaming is the point in CI.
    fn emit(&mut self, event: &WireEvent) -> std::io::Result<()> {
        write_line(self.out, event)
    }
}

/// Serializes one event as a single NDJSON line and flushes immediately.
fn write_line(out: &mut impl std::io::Write, event: &WireEvent) -> std::io::Result<()> {
    let mut line = serde_json::to_string(&Wire {
        schema: SCHEMA,
        event,
    })?;
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()
}

/// Wraps an event with the schema field without threading `schema` through
/// every variant.
#[derive(Serialize)]
struct Wire<'a> {
    schema: u32,
    #[serde(flatten)]
    event: &'a WireEvent,
}

/// A broken pipe means the consumer closed stdout (the common `... | head`
/// pattern): a normal, expected end of the stream, not a failure. Returns
/// `Ok(true)` to stop emitting cleanly, propagates any other I/O error, and
/// returns `Ok(false)` to keep going. Reporting it as an error would print a
/// non-JSON line to stderr and exit non-zero, breaking the `--json` contract.
fn stop_on_broken_pipe(result: std::io::Result<()>) -> std::io::Result<bool> {
    match result {
        Ok(()) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(true),
        Err(e) => Err(e),
    }
}

#[async_trait]
impl<W: Write + Send> Reporter for JsonReporter<'_, W> {
    async fn run(&mut self, mut rx: mpsc::Receiver<SchedulerEvent>) -> std::io::Result<bool> {
        let started = Instant::now();
        let mut overall = true;

        let run_started = WireEvent::RunStarted {
            target: self.target.clone(),
            beams: self.beams.clone(),
            at: now_iso8601(),
        };
        if stop_on_broken_pipe(self.emit(&run_started))? {
            return Ok(overall);
        }

        while let Some(event) = rx.recv().await {
            let stop = match event {
                SchedulerEvent::BeamStarted { name } => {
                    stop_on_broken_pipe(self.emit(&WireEvent::BeamStarted {
                        beam: name,
                        at: now_iso8601(),
                    }))?
                }
                SchedulerEvent::BeamOutput {
                    name,
                    line,
                    is_stderr,
                } => {
                    let stream = if is_stderr { "stderr" } else { "stdout" };
                    stop_on_broken_pipe(self.emit(&WireEvent::BeamOutput {
                        beam: name,
                        stream,
                        line,
                    }))?
                }
                SchedulerEvent::BeamCompleted { name, status } => {
                    let (status, duration_ms) = map_status(status);
                    stop_on_broken_pipe(self.emit(&WireEvent::BeamCompleted {
                        beam: name,
                        status,
                        duration_ms,
                        at: now_iso8601(),
                    }))?
                }
                SchedulerEvent::AllDone { success } => {
                    overall = success;
                    let elapsed: Duration = started.elapsed();
                    let _ = stop_on_broken_pipe(self.emit(&WireEvent::RunCompleted {
                        success,
                        duration_ms: elapsed.as_millis(),
                        at: now_iso8601(),
                    }))?;
                    break;
                }
            };
            if stop {
                return Ok(overall);
            }
        }

        Ok(overall)
    }
}

/// Serializes a pre-run failure as a single `error` line. Used by `main` when
/// the run cannot start under `--json` (invalid Beamfile, unknown target, ...).
pub fn write_error(out: &mut impl Write, kind: &str, message: &str) -> std::io::Result<()> {
    let event = WireEvent::Error {
        kind: kind.to_string(),
        message: message.to_string(),
    };
    write_line(out, &event)
}
