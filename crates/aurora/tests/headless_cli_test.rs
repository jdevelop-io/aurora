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
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--no-tui"])
        .current_dir(dir.path())
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
}

#[test]
fn dry_run_prints_execution_plan_by_level() {
    let beamfile = r#"
beam "composer" { run { commands = ["echo c"] } }
beam "lint" { depends_on = ["composer"] run { commands = ["echo l"] } }
beam "test" { depends_on = ["composer"] run { commands = ["echo t"] } }
beam "qa"   { depends_on = ["lint", "test"] }
"#;
    let dir = fixture_dir(beamfile);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["qa", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "dry-run must exit 0:\n{stdout}");
    assert!(
        stdout.contains("level 0: composer"),
        "plan must show level 0 first:\n{stdout}"
    );
    assert!(
        stdout.contains("qa"),
        "plan must include the target:\n{stdout}"
    );
}

#[test]
fn warns_when_beamfile_comes_from_parent_directory() {
    let parent = fixture_dir(BEAMFILE);
    let sub = parent.path().join("subdir");
    fs::create_dir_all(&sub).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--no-tui"])
        .current_dir(&sub)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "should still run from a subdirectory"
    );
    assert!(
        stderr.contains("parent directory"),
        "must warn that the Beamfile came from a parent directory:\n{stderr}"
    );
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
    let dir = fixture_dir(BROKEN_BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["broken", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 on DAG error\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

const CYCLIC_BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "a"
}

beam "a" {
  depends_on = ["b"]
  run { commands = ["echo a"] }
}

beam "b" {
  depends_on = ["a"]
  run { commands = ["echo b"] }
}
"#;

#[test]
fn dependency_cycle_exits_one() {
    let dir = fixture_dir(CYCLIC_BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["a", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 on a dependency cycle\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn failing_beam_exits_one() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["boom", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1\nstdout:\n{stdout}"
    );
    assert!(stdout.contains("[FAIL]"), "recap fail marker:\n{stdout}");
}

/// A Beamfile whose beam declares `inputs`, so a normal run persists a
/// cache entry. Cache persistence requires non-empty `inputs` and the input
/// file present on disk, so `cacheable_dir` also writes that file.
const CACHEABLE_BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "cached"
}

beam "cached" {
  description = "beam with declared inputs"
  inputs = ["input.txt"]
  run { commands = ["echo hello"] }
}
"#;

fn cacheable_dir() -> TempDir {
    let dir = fixture_dir(CACHEABLE_BEAMFILE);
    fs::write(dir.path().join("input.txt"), "content").unwrap();
    dir
}

#[test]
fn no_cache_writes_no_cache_directory() {
    let dir = cacheable_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["cached", "--no-tui", "--no-cache"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "run should succeed");
    assert!(
        !dir.path().join(".aurora/cache").exists(),
        "--no-cache must not persist a cache directory"
    );
}

#[test]
fn default_run_writes_cache_directory() {
    let dir = cacheable_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["cached", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "run should succeed");
    assert!(
        dir.path().join(".aurora/cache").exists(),
        "a normal run persists a cache directory (proving --no-cache matters)"
    );
}
