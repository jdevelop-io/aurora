//! One real-notify integration test for headless watch mode: touch a watched
//! input and assert a second cycle runs, then interrupt and assert exit 0.
//! Real filesystem events are slow, so timeouts are generous.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

// The beam appends a line to runs.log on every real run. A cache hit would NOT
// run the command, so re-running proves the input change invalidated the cache.
const BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "work"
}

beam "work" {
  inputs = ["watched.txt"]
  run { commands = ["echo run >> runs.log"] }
}
"#;

fn count_lines(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn a_change_triggers_a_second_cycle_then_ctrl_c_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Beamfile"), BEAMFILE).unwrap();
    fs::write(dir.path().join("watched.txt"), "v1").unwrap();
    let runs = dir.path().join("runs.log");

    let mut child = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["work", "--no-tui", "--watch"])
        .current_dir(dir.path())
        .spawn()
        .unwrap();

    assert!(
        wait_until(|| count_lines(&runs) >= 1, Duration::from_secs(20)),
        "the first cycle never ran"
    );

    // Change the watched input: cache miss -> the command runs again.
    fs::write(dir.path().join("watched.txt"), "v2").unwrap();

    assert!(
        wait_until(|| count_lines(&runs) >= 2, Duration::from_secs(20)),
        "a change to a watched input did not trigger a second cycle"
    );

    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success(), "failed to signal the aurora process");

    let status = child.wait().unwrap();
    assert_eq!(
        status.code(),
        Some(0),
        "leaving watch mode with Ctrl-C exits 0"
    );
}
