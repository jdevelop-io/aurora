use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn fixture_dir(beamfile: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Beamfile"), beamfile).unwrap();
    dir
}

const BEAMFILE: &str = r#"
aurora { version = "1"  default = "greet" }

beam "greet" {
  run { commands = ["echo hello ${arg.1}"] }
}

beam "passthrough" {
  run { commands = ["echo got:${args}"] }
}

beam "needy" {
  run { commands = ["echo ${arg.1}"] }
}
"#;

#[test]
fn positional_argument_reaches_the_target() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["greet", "Alice", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "run failed:\n{stdout}");
    assert!(
        stdout.contains("hello Alice"),
        "argument not applied:\n{stdout}"
    );
}

#[test]
fn double_dash_forwards_a_hyphen_leading_tail() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["passthrough", "--no-tui", "--", "--flag", "value"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "run failed:\n{stdout}");
    assert!(
        stdout.contains("got:--flag value"),
        "tail not forwarded:\n{stdout}"
    );
}

#[test]
fn missing_argument_fails_before_running() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["needy", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(0),
        "must fail on a missing argument"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing argument"),
        "error must name the missing argument:\n{stderr}"
    );
}
