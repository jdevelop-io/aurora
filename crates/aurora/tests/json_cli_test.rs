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

#[test]
fn cyclic_beamfile_emits_error_event_on_stdout() {
    let beamfile = r#"
beam "a" { depends_on = ["b"] run { commands = ["echo a"] } }
beam "b" { depends_on = ["a"] run { commands = ["echo b"] } }
"#;
    let dir = fixture_dir(beamfile);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["a", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "cycle must exit 1");
    assert!(
        stderr.is_empty(),
        "nothing on stderr in --json mode:\n{stderr}"
    );
    let last = stdout.lines().last().expect("an error line on stdout");
    let value: serde_json::Value = serde_json::from_str(last).unwrap();
    assert_eq!(value["event"], "error");
    assert_eq!(value["schema"], 1);
    assert!(
        value["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("cycle"),
        "message names the cycle: {last}"
    );
}

#[test]
fn failing_environment_block_emits_error_event_on_stdout() {
    let beamfile = r#"
environment { BROKEN = shell("false") }
beam "ok" { run { commands = ["echo hi"] } }
"#;
    let dir = fixture_dir(beamfile);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "env failure must exit 1");
    assert!(
        stderr.is_empty(),
        "nothing on stderr in --json mode:\n{stderr}"
    );
    let last = stdout.lines().last().expect("an error line on stdout");
    let value: serde_json::Value = serde_json::from_str(last).unwrap();
    assert_eq!(value["event"], "error");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "beamfile");
}
