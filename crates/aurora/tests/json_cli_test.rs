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
aurora { version = "1"  default = "ok" }
beam "ok"   { run { commands = ["echo hello"] } }
beam "boom" { run { commands = ["exit 3;"] } }
"#;

/// Parses every non-empty stdout line as JSON, panicking on the first that is not.
fn parse_lines(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}\n{e}")))
        .collect()
}

#[test]
fn passing_run_is_all_json_on_stdout_and_stderr_is_empty() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "exit: {:?}", output.status.code());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "stderr must be empty in --json mode:\n{stderr}"
    );

    let lines = parse_lines(&stdout);
    assert_eq!(lines.first().unwrap()["event"], "run_started");
    assert_eq!(lines.last().unwrap()["event"], "run_completed");
    assert_eq!(lines.last().unwrap()["success"], true);
    // The command output is carried as a beam_output event, not raw text.
    assert!(
        lines
            .iter()
            .any(|l| l["event"] == "beam_output" && l["line"] == "hello"),
        "command output present as an event:\n{stdout}"
    );
}

#[test]
fn failing_beam_reports_failed_status_and_exits_one() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["boom", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = parse_lines(&stdout);
    let completed = lines
        .iter()
        .find(|l| l["event"] == "beam_completed" && l["beam"] == "boom")
        .unwrap();
    assert_eq!(completed["status"], "failed");
    assert_eq!(completed["exit_code"], 3);
    assert_eq!(lines.last().unwrap()["success"], false);
}

#[test]
fn json_conflicts_with_interactive() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--json", "-i"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success(), "clap must reject --json with -i");
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

#[test]
fn run_started_reports_only_the_dependency_closure() {
    let beamfile = r#"
beam "target" { depends_on = ["dep"] run { commands = ["echo t"] } }
beam "dep" { run { commands = ["echo d"] } }
beam "unrelated" { run { commands = ["echo u"] } }
"#;
    let dir = fixture_dir(beamfile);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["target", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().expect("a run_started line");
    let value: serde_json::Value = serde_json::from_str(first).unwrap();
    assert_eq!(value["event"], "run_started");
    let beams: Vec<String> = value["beams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_string())
        .collect();
    assert!(
        beams.contains(&"target".to_string()),
        "closure has the target: {beams:?}"
    );
    assert!(
        beams.contains(&"dep".to_string()),
        "closure has the dependency: {beams:?}"
    );
    assert!(
        !beams.contains(&"unrelated".to_string()),
        "closure excludes unrelated beams: {beams:?}"
    );
}
