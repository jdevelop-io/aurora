//! A target or a `--var` key that does not exist in the Beamfile must fail the
//! run. Silently doing nothing and exiting 0 turns a typo in a CI pipeline into
//! a green build.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn fixture_dir(beamfile: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Beamfile"), beamfile).unwrap();
    dir
}

const BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "build"
}

variable "image" {
  default = "alpine:3"
}

beam "build" {
  description = "build the project"
  run { commands = ["echo building ${var.image}"] }
}
"#;

fn run(dir: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(args)
        .current_dir(dir.path())
        .output()
        .unwrap()
}

#[test]
fn unknown_target_exits_one() {
    let dir = fixture_dir(BEAMFILE);
    let output = run(&dir, &["buidl", "--no-tui"]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "an unknown target must fail the run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unknown_target_suggests_the_closest_beam() {
    let dir = fixture_dir(BEAMFILE);
    let output = run(&dir, &["buidl", "--no-tui"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("buidl") && stderr.contains("build"),
        "the error must name the unknown target and suggest the closest beam:\n{stderr}"
    );
}

#[test]
fn unknown_target_exits_one_in_dry_run() {
    let dir = fixture_dir(BEAMFILE);
    let output = run(&dir, &["buidl", "--dry-run"]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "--dry-run must validate the target too\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unknown_default_beam_exits_one() {
    let beamfile = r#"
aurora {
  version = "1"
  default = "ghost"
}

beam "build" {
  run { commands = ["echo building"] }
}
"#;
    let dir = fixture_dir(beamfile);
    let output = run(&dir, &["--no-tui"]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a `default` pointing at a missing beam must fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unknown_var_key_exits_one_and_suggests() {
    let dir = fixture_dir(BEAMFILE);
    let output = run(&dir, &["build", "--no-tui", "--var", "imge=alpine:edge"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "an unknown --var key must fail the run\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout),
    );
    assert!(
        stderr.contains("imge") && stderr.contains("image"),
        "the error must name the unknown key and suggest the closest variable:\n{stderr}"
    );
}

#[test]
fn known_target_and_var_still_run() {
    let dir = fixture_dir(BEAMFILE);
    let output = run(&dir, &["build", "--no-tui", "--var", "image=alpine:edge"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "a valid target and a valid --var key must still run\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("building alpine:edge"),
        "the override must reach the command:\n{stdout}"
    );
}
