//! CLI tests for parameterized beams: binding CLI arguments (positional and
//! named) to `param "..." {}` declarations, the `--list` signatures, and the
//! `--dry-run` instance plan. Supersedes the old `${arg.N}` tests removed from
//! `args_cli_test.rs`.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Creates a temporary working directory holding a Beamfile. The returned
/// [`TempDir`] removes the directory when dropped, including on a test panic.
fn fixture_dir(beamfile: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Beamfile"), beamfile).unwrap();
    dir
}

const BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "deploy"
}

beam "build" {
  param "version" {}
  run { commands = ["echo building ${param.version}"] }
}

beam "deploy" {
  param "version" {}
  param "env" { default = "staging" }
  depends_on = [{ beam = "build", params = { version = "${param.version}" } }]
  run { commands = ["echo deploy ${param.version} to ${param.env}"] }
}
"#;

#[test]
fn positional_and_named_binding_reaches_target_and_dependency() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["--no-tui", "deploy", "1.2.3", "env=prod"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "run failed:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("building 1.2.3"),
        "the dependency did not receive the bound param:\n{stdout}"
    );
    assert!(
        stdout.contains("deploy 1.2.3 to prod"),
        "the target did not receive its bound params:\n{stdout}"
    );
}

#[test]
fn missing_required_param_exits_one_with_signature() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["--no-tui", "deploy"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "must exit 1 when a required param is missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing required param 'version'"),
        "error must name the missing param:\n{stderr}"
    );
    assert!(
        stderr.contains("deploy <version> [env=staging]"),
        "error must show the usage signature:\n{stderr}"
    );
}

#[test]
fn list_shows_param_signatures() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["--list"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "--list must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("deploy <version> [env=staging]"),
        "--list must show the param signature:\n{stdout}"
    );
}

#[test]
fn dry_run_prints_instance_ids() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["--dry-run", "deploy", "1.2.3"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "--dry-run must exit 0:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("build[version=1.2.3]"),
        "plan must show the dependency's instance id:\n{stdout}"
    );
    assert!(
        stdout.contains("deploy[env=staging,version=1.2.3]"),
        "plan must show the target's instance id:\n{stdout}"
    );
}
