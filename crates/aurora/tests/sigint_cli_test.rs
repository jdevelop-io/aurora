//! Ctrl-C on a headless run must not leave the beam's process subtree alive.
//!
//! Beams run in their own process group (so cancellation can reap the whole
//! subtree), which also means they do not receive the terminal's SIGINT. If
//! Aurora dies on the default disposition, its `Drop`-based process-group
//! cleanup never runs and the commands keep going after Aurora is gone.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// The beam announces its start, sleeps, then writes a second marker. The
/// `survived` marker is the witness: if it appears after Aurora was
/// interrupted, the child outlived its parent.
const BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "slow"
}

beam "slow" {
  description = "announces itself, sleeps, then leaves a trace"
  run { commands = ["touch started.marker && sleep 3 && touch survived.marker"] }
}
"#;

/// Waits for `path` to appear. Polling the beam's own start marker keeps the
/// test off a fixed sleep: interrupting Aurora before it has even spawned the
/// command would prove nothing (and silently pass).
fn wait_for(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn sigint_kills_the_beam_subtree_and_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Beamfile"), BEAMFILE).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["slow", "--no-tui"])
        .current_dir(dir.path())
        .spawn()
        .unwrap();

    assert!(
        wait_for(&dir.path().join("started.marker"), Duration::from_secs(20)),
        "the beam never started, so this test would prove nothing"
    );

    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success(), "failed to signal the aurora process");

    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "an interrupted run must not report success"
    );

    // Outlive the beam's own sleep: if the subtree survived, the marker lands.
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        !dir.path().join("survived.marker").exists(),
        "the beam's process subtree outlived the interrupted run"
    );
}
