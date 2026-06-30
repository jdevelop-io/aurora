use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Crée un répertoire de travail temporaire avec un Beamfile et le renvoie.
fn fixture_dir(tag: &str, beamfile: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aurora-headless-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Beamfile"), beamfile).unwrap();
    dir
}

const BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "ok"
}

beam "ok" {
  description = "passing beam"
  run { commands = ["echo hello"] }
}

beam "boom" {
  description = "failing beam"
  run { commands = ["exit 3"] }
}
"#;

#[test]
fn passing_beam_streams_prefixed_output_and_exits_zero() {
    let dir = fixture_dir("ok", BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--no-tui"])
        .current_dir(&dir)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit status: {:?}\nstdout:\n{stdout}",
        output.status.code()
    );
    assert!(
        stdout.contains("hello"),
        "command output streamed:\n{stdout}"
    );
    assert!(stdout.contains("[ok"), "per-beam prefix present:\n{stdout}");
    assert!(stdout.contains("[PASS]"), "recap pass marker:\n{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

const BROKEN_BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "broken"
}

beam "broken" {
  description = "depends on a missing beam"
  depends_on = ["does_not_exist"]
  run { commands = ["echo nope"] }
}
"#;

#[test]
fn unknown_dependency_exits_one() {
    let dir = fixture_dir("broken", BROKEN_BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["broken", "--no-tui"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 on DAG error\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failing_beam_exits_one() {
    let dir = fixture_dir("boom", BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["boom", "--no-tui"])
        .current_dir(&dir)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1\nstdout:\n{stdout}"
    );
    assert!(stdout.contains("[FAIL]"), "recap fail marker:\n{stdout}");
    let _ = fs::remove_dir_all(&dir);
}
