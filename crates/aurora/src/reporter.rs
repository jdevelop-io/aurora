//! The rendering axis for non-interactive runs.
//!
//! A `Reporter` drains the scheduler's event stream and renders it to some
//! sink, returning the overall success that drives the exit code. Headless
//! (human-oriented text) and JSON (a machine contract) are the two concrete
//! renderers today; JUnit XML or CI annotations would be further ones. The TUI
//! is deliberately not a `Reporter`: it owns the terminal, the cancellation
//! channel and the rerun closure, and would not fit behind this interface.

use async_trait::async_trait;
use aurora_core::events::SchedulerEvent;
use tokio::sync::mpsc;

#[async_trait]
pub trait Reporter: Send {
    /// Drains the event stream to completion, rendering as it goes. Returns the
    /// overall success carried by `AllDone`, which drives the process exit code.
    async fn run(&mut self, rx: mpsc::Receiver<SchedulerEvent>) -> std::io::Result<bool>;
}
