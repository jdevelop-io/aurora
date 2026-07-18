//! Regression test for the reload path of headless `--watch` with a
//! parameterized target.
//!
//! The scheduler and DAG are keyed by the *instance id* (`work[flavor=a]`),
//! never by the raw beam name (`work`). On a Beamfile reload the watch loop
//! re-derives `RunInputs` (which now carries a `target_id`), and must use
//! that refreshed id as the scheduler root. Using the raw target name, or a
//! `target_id` captured once before the loop and never refreshed, means that
//! as soon as a Beamfile edit changes an unbound param's default (so the
//! instance id changes, e.g. `work[flavor=a]` -> `work[flavor=b]`), the stale
//! id no longer names any beam in the freshly expanded instance list. The
//! scheduler's root is then unknown, its transitive closure is empty, and the
//! cycle silently runs zero beams instead of erroring or re-running the beam.
//!
//! This test proves the fix end-to-end: it starts `--watch` on a target with
//! an unbound, defaulted param, waits for the first cycle to run with the
//! default value, edits the Beamfile to change that default (changing the
//! instance id without touching the CLI invocation at all), and asserts a
//! second cycle actually runs with the new value. Real filesystem events are
//! slow, so timeouts are generous, matching `watch_headless_cli_test.rs`.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

// `flavor` is never bound on the command line, so its value always comes from
// the beam's own default: exactly the case where a Beamfile edit changes the
// instance id (`work[flavor=<default>]`) without any change to argv.
const BEAMFILE_V1: &str = r#"
aurora {
  version = "1"
  default = "work"
}

beam "work" {
  param "flavor" { default = "a" }
  inputs = ["watched.txt"]
  run { commands = ["echo run-${param.flavor} >> runs.log"] }
}
"#;

const BEAMFILE_V2: &str = r#"
aurora {
  version = "1"
  default = "work"
}

beam "work" {
  param "flavor" { default = "b" }
  inputs = ["watched.txt"]
  run { commands = ["echo run-${param.flavor} >> runs.log"] }
}
"#;

fn read_runs(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
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
fn a_beamfile_edit_that_changes_the_instance_id_still_reruns_on_reload() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Beamfile"), BEAMFILE_V1).unwrap();
    fs::write(dir.path().join("watched.txt"), "v1").unwrap();
    let runs = dir.path().join("runs.log");

    // No CLI argument binds `flavor`: it always comes from the Beamfile's own
    // default, which is exactly what changes between v1 and v2 below.
    let mut child = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["work", "--no-tui", "--watch"])
        .current_dir(dir.path())
        .spawn()
        .unwrap();

    assert!(
        wait_until(
            || read_runs(&runs).iter().any(|l| l == "run-a"),
            Duration::from_secs(20)
        ),
        "the first cycle never ran with the default 'a' binding:\n{:?}",
        read_runs(&runs)
    );

    // Change the beam's own default value: the instance id moves from
    // `work[flavor=a]` to `work[flavor=b]`, with no change to the CLI
    // invocation and no touch of the watched input file. Only the Beamfile
    // change should be needed to trigger the next cycle (it is always
    // watched, regardless of the (possibly stale) watch closure).
    fs::write(dir.path().join("Beamfile"), BEAMFILE_V2).unwrap();

    assert!(
        wait_until(
            || read_runs(&runs).iter().any(|l| l == "run-b"),
            Duration::from_secs(20)
        ),
        "a Beamfile edit that changes the target's instance id must still \
         schedule the beam on reload, not silently run zero beams:\n{:?}",
        read_runs(&runs)
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
