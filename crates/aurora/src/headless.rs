//! Text rendering for headless mode: drains the scheduler's event stream,
//! displays it as lines prefixed by beam (stdout/stderr separated), then prints
//! a final recap. Returns the overall success, which drives the exit code.

use std::io::Write;

use aurora_core::events::{BeamStatus, SchedulerEvent, SkipReason};
use tokio::sync::mpsc;

use crate::reporter::Reporter;
use async_trait::async_trait;

/// Human-oriented text renderer: prefixed per-beam lines then an ASCII recap.
pub struct HeadlessReporter<'a, O: std::io::Write, E: std::io::Write> {
    beam_names: Vec<String>,
    out_color: bool,
    err_color: bool,
    out: &'a mut O,
    err: &'a mut E,
}

impl<'a, O: std::io::Write, E: std::io::Write> HeadlessReporter<'a, O, E> {
    pub fn new(
        beam_names: Vec<String>,
        out_color: bool,
        err_color: bool,
        out: &'a mut O,
        err: &'a mut E,
    ) -> Self {
        Self {
            beam_names,
            out_color,
            err_color,
            out,
            err,
        }
    }
}

#[async_trait]
impl<O: std::io::Write + Send, E: std::io::Write + Send> Reporter for HeadlessReporter<'_, O, E> {
    async fn run(&mut self, rx: mpsc::Receiver<SchedulerEvent>) -> std::io::Result<bool> {
        render_headless(
            &self.beam_names,
            self.out_color,
            self.err_color,
            rx,
            self.out,
            self.err,
        )
        .await
    }
}

/// Wraps `text` in an ANSI color code when `use_color` is true.
fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    } else {
        text.to_string()
    }
}

/// Formats a duration in seconds with one decimal place (e.g. "4.2s").
fn fmt_duration(d: std::time::Duration) -> String {
    format!("{:.1}s", d.as_secs_f64())
}

/// Builds the recap line for a completed beam.
/// Returns `None` for non-terminal statuses (Pending/Running), never emitted here.
fn recap_line(name: &str, status: &BeamStatus, width: usize, use_color: bool) -> Option<String> {
    let (marker, color, detail) = match status {
        BeamStatus::Success {
            duration,
            cached: false,
        } => ("PASS", "32", fmt_duration(*duration)),
        BeamStatus::Success { cached: true, .. } => ("PASS", "32", "cached".to_string()),
        BeamStatus::Skipped { reason } => {
            let r = match reason {
                SkipReason::Cached => "cached",
                SkipReason::SkipIf => "skip_if",
                SkipReason::ConditionNotMet => "condition not met",
            };
            ("SKIP", "33", r.to_string())
        }
        BeamStatus::Failed {
            exit_code,
            duration,
        } => (
            "FAIL",
            "31",
            format!("exit {exit_code} {}", fmt_duration(*duration)),
        ),
        BeamStatus::FailedAllowed {
            exit_code,
            duration,
        } => (
            "WARN",
            "33",
            format!("exit {exit_code} (allowed) {}", fmt_duration(*duration)),
        ),
        BeamStatus::Cancelled => ("CANC", "35", "cancelled".to_string()),
        BeamStatus::Pending | BeamStatus::Running => return None,
    };
    // Marker padded to 6 characters ("[FAIL]") before coloring, so the columns
    // stay aligned without the ANSI codes (zero width) shifting them.
    let marker = format!("{:<6}", format!("[{marker}]"));
    let marker = paint(&marker, color, use_color);
    Some(format!("{marker} {name:<width$}  {detail}"))
}

/// Drains the event stream until `AllDone`, prints the prefixed lines
/// then the recap. Returns the overall success carried by `AllDone`.
///
/// Color is decided per target output stream (`out_color` for stdout,
/// `err_color` for stderr) rather than globally: a `2>file` redirection must not
/// inherit the color decided for stdout (and vice versa).
async fn render_headless(
    beam_names: &[String],
    out_color: bool,
    err_color: bool,
    mut rx: mpsc::Receiver<SchedulerEvent>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<bool> {
    let width = beam_names.iter().map(|n| n.len()).max().unwrap_or(0);
    let mut recap: Vec<(String, BeamStatus)> = Vec::new();
    let mut overall = true;

    while let Some(event) = rx.recv().await {
        match event {
            SchedulerEvent::BeamOutput {
                name,
                line,
                is_stderr,
            } => {
                let color = if is_stderr { err_color } else { out_color };
                let prefix = paint(&format!("[{name:<width$}]"), "90", color);
                if is_stderr {
                    writeln!(err, "{prefix} {line}")?;
                } else {
                    writeln!(out, "{prefix} {line}")?;
                }
            }
            SchedulerEvent::BeamCompleted { name, status } => recap.push((name, status)),
            SchedulerEvent::BeamStarted { .. } => {}
            SchedulerEvent::AllDone { success } => {
                overall = success;
                break;
            }
        }
    }

    writeln!(out)?;
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut cancelled = 0usize;
    for (name, status) in &recap {
        if let Some(line) = recap_line(name, status, width, out_color) {
            writeln!(out, "{line}")?;
        }
        // Like the interactive runner: `cancelled` is a neutral category,
        // never counted as a failure. Only a beam that actually fails
        // (outside allow_failure) feeds `failed`.
        match status {
            BeamStatus::Success { .. }
            | BeamStatus::Skipped { .. }
            | BeamStatus::FailedAllowed { .. } => ok += 1,
            BeamStatus::Failed { .. } => failed += 1,
            BeamStatus::Cancelled => cancelled += 1,
            BeamStatus::Pending | BeamStatus::Running => {}
        }
    }
    // The `cancelled` category only appears when it is nonzero, to
    // keep the recap of a clean run readable.
    let mut summary = format!("Done: {ok} ok, {failed} failed");
    if cancelled > 0 {
        summary.push_str(&format!(", {cancelled} cancelled"));
    }
    writeln!(out, "{summary}")?;

    Ok(overall)
}
