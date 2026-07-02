# Per-beam working directory (`dir`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a beam declare `dir = "..."` so its run commands, inputs/outputs, and gates all resolve against that directory instead of the Beamfile directory.

**Architecture:** `dir` is parsed into a new `Beam.dir: Option<String>` field. The scheduler overrides the single `working_dir` value it already threads through gating, cache input-resolution, and `ExecutionInput`, so every downstream consumer inherits the beam's directory with no extra plumbing. Executors are unchanged (they already consume `input.working_dir`).

**Tech Stack:** Rust, `pest` grammar, `tokio`, integration tests in each crate's `tests/`.

## Global Constraints

- All code comments and commit messages in English.
- Commits: gitmoji + Conventional Commits (e.g. `:sparkles: feat(parser): ...`). No Claude/Anthropic attribution anywhere.
- `cargo clippy --workspace -- -D warnings` must stay clean (warnings are errors).
- Tests are integration tests under each crate's `tests/` directory (exercise the public crate API), never `#[cfg(test)]` inline modules.
- Semantics ("beam root"): a relative `dir` joins onto the Beamfile directory; an absolute `dir` replaces it (`Path::join` semantics). The cache *store* stays at `<repo-root>/.aurora/cache`; only input/output *resolution* moves under `dir`. The global `environment {}` block is unaffected.
- Reference spec: `docs/superpowers/specs/2026-07-02-per-beam-dir-design.md`.

---

### Task 1: Parse `dir` into the AST

**Files:**
- Modify: `crates/aurora-core/src/parser/aurora.pest` (beam grammar)
- Modify: `crates/aurora-core/src/ast.rs:44-54` (`Beam` struct)
- Modify: `crates/aurora-core/src/parser/mod.rs:236-280` (`parse_beam_block`)
- Modify: every `Beam { .. }` literal (compiler-guided — see Step 4)
- Test: `crates/aurora-core/tests/parser_test.rs`

**Interfaces:**
- Produces: `Beam.dir: Option<String>` — `Some(path)` when the beam declares `dir = "path"`, `None` otherwise. Consumed by Task 2 (interpolation) and Task 3 (scheduler).

- [ ] **Step 1: Write the failing test**

Append to `crates/aurora-core/tests/parser_test.rs`:

```rust
#[test]
fn test_parse_beam_dir() {
    let input = r#"
beam "build" {
  dir = "packages/api"
  run {
    commands = ["npm run build"]
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams[0].dir.as_deref(), Some("packages/api"));
}

#[test]
fn test_parse_beam_without_dir_is_none() {
    let input = r#"
beam "build" {
  run {
    commands = ["npm run build"]
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams[0].dir, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p aurora-core --test parser_test test_parse_beam_dir`
Expected: FAIL — compile error `no field 'dir' on type '&Beam'`.

- [ ] **Step 3: Add the grammar rule**

In `crates/aurora-core/src/parser/aurora.pest`, add `beam_dir` to the `beam_field` alternation and define the rule alongside the other scalar fields:

```pest
beam_field = {
    beam_description |
    beam_depends_on  |
    beam_inputs      |
    beam_outputs     |
    beam_dir         |
    beam_skip_if     |
    beam_allow_failure |
    beam_condition   |
    beam_run
}
beam_description = { "description" ~ "=" ~ string }
beam_depends_on  = { "depends_on"  ~ "=" ~ string_list }
beam_inputs      = { "inputs"      ~ "=" ~ string_list }
beam_outputs     = { "outputs"     ~ "=" ~ string_list }
beam_dir         = { "dir"         ~ "=" ~ string }
beam_skip_if     = { "skip_if"     ~ "=" ~ string }
beam_allow_failure = { "allow_failure" ~ "=" ~ bool }
```

- [ ] **Step 4: Add the AST field and fix every `Beam` literal**

In `crates/aurora-core/src/ast.rs`, add the field to `Beam` (placed after `outputs` to mirror the grammar order):

```rust
#[derive(Debug, Clone)]
pub struct Beam {
    pub name: String,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Working directory for this beam. When set, the beam's run commands,
    /// inputs/outputs and gates all resolve against this directory. Relative
    /// paths join onto the Beamfile directory; absolute paths replace it.
    pub dir: Option<String>,
    pub skip_if: Option<String>,
    pub condition: Option<Condition>,
    pub run: Option<Run>,
    pub allow_failure: bool,
}
```

In `crates/aurora-core/src/parser/mod.rs`, initialise the field in `parse_beam_block` (line ~236):

```rust
    let mut beam = Beam {
        name,
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: None,
        allow_failure: false,
    };
```

and handle the new rule inside the field `match` (next to `beam_outputs`):

```rust
            Rule::beam_dir => {
                beam.dir = Some(unquote(field.into_inner().next().unwrap()));
            }
```

Then run `cargo build --workspace` and add `dir: None,` to every other `Beam { .. }` literal the compiler flags. Known sites: `crates/aurora/src/main.rs:120` (the `virtual_beam`) and the test helpers/literals in `crates/aurora-core/tests/`: `scheduler_test.rs`, `scheduler_cache_test.rs`, `scheduler_cancel_test.rs`, `scheduler_condition_test.rs`, `scheduler_panic_test.rs`, `scheduler_parallelism_test.rs`, `scheduler_rerun_test.rs`, `scheduler_executor_error_test.rs`, `scheduler_semaphore_cancel_test.rs`, `scheduler_unknown_executor_test.rs`, `scheduler_skip_if_test.rs`, `ast_test.rs`. The compiler lists each missing-field error; the build is green only once all are fixed.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p aurora-core --test parser_test test_parse_beam`
Expected: PASS (`test_parse_beam_dir` and `test_parse_beam_without_dir_is_none`).

- [ ] **Step 6: Run the full core suite and clippy**

Run: `cargo test -p aurora-core && cargo clippy --workspace -- -D warnings`
Expected: all tests pass, no clippy warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/aurora-core/src/parser/aurora.pest crates/aurora-core/src/ast.rs crates/aurora-core/src/parser/mod.rs crates/aurora-core/tests crates/aurora/src/main.rs
git commit -m ":sparkles: feat(parser): parse per-beam dir field"
```

---

### Task 2: Interpolate `${var.name}` in `dir`

**Files:**
- Modify: `crates/aurora-core/src/parser/mod.rs:65-94` (`resolve_variables`)
- Test: `crates/aurora-core/tests/parser_test.rs`

**Interfaces:**
- Consumes: `Beam.dir` (Task 1), the existing `interpolate_command(s: &str, vars: &HashMap<String, String>, beam: &str) -> Result<String>` helper.
- Produces: after `resolve_variables`, `Beam.dir` has its `${var.name}` tokens expanded; an unknown variable is a hard error (same as commands).

- [ ] **Step 1: Write the failing test**

Append to `crates/aurora-core/tests/parser_test.rs`:

```rust
#[test]
fn test_dir_interpolates_variables() {
    let input = r#"
variable "pkg" {
  default = "api"
}
beam "build" {
  dir = "packages/${var.pkg}"
  run {
    commands = ["npm run build"]
  }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(bf.beams[0].dir.as_deref(), Some("packages/api"));
}

#[test]
fn test_dir_unknown_variable_is_error() {
    let input = r#"
beam "build" {
  dir = "packages/${var.missing}"
  run {
    commands = ["npm run build"]
  }
}
"#;
    let mut bf = parse(input).unwrap();
    assert!(resolve_variables(&mut bf).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p aurora-core --test parser_test test_dir_interpolates_variables test_dir_unknown_variable_is_error`
Expected: FAIL — `test_dir_interpolates_variables` asserts `Some("packages/api")` but gets the un-expanded `Some("packages/${var.pkg}")`; `test_dir_unknown_variable_is_error` gets `Ok` instead of `Err`.

- [ ] **Step 3: Interpolate `dir` in `resolve_variables`**

In `crates/aurora-core/src/parser/mod.rs`, inside the `for beam in &mut beam_file.beams` loop of `resolve_variables`, add a block that runs regardless of whether the beam has a `run` (place it right after `let beam_name = beam.name.clone();`):

```rust
        if let Some(dir) = &mut beam.dir {
            *dir = interpolate_command(dir, &vars, &beam_name)?;
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p aurora-core --test parser_test test_dir_interpolates_variables test_dir_unknown_variable_is_error`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aurora-core/src/parser/mod.rs crates/aurora-core/tests/parser_test.rs
git commit -m ":sparkles: feat(parser): interpolate variables in beam dir"
```

---

### Task 3: Apply `dir` as the beam working directory

**Files:**
- Modify: `crates/aurora-core/src/scheduler.rs:363-449` (`run_beam_task`, right after the aggregation-node early return)
- Test: `crates/aurora-core/tests/scheduler_dir_test.rs` (new)

**Interfaces:**
- Consumes: `Beam.dir` (Task 1), the `working_dir: PathBuf` destructured from `TaskEnv`.
- Produces: within `run_beam_task`, a shadowed `working_dir` that already feeds `gate_skip_reason`, `cache_lookup_blocking`, and `ExecutionInput.working_dir`. No signature changes.

- [ ] **Step 1: Write the failing test**

Create `crates/aurora-core/tests/scheduler_dir_test.rs`:

```rust
use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert("local".into(), Arc::new(LocalExecutor::new()));
    m
}

fn output_lines(events: &[SchedulerEvent], beam: &str) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            SchedulerEvent::BeamOutput { name, line, .. } if name == beam => Some(line.clone()),
            _ => None,
        })
        .collect()
}

/// A beam whose `dir` points at a subdirectory runs its commands there: a
/// relative `cat` only resolves when the process cwd is that subdirectory.
#[tokio::test]
async fn dir_sets_command_working_directory() {
    let root = tempfile::tempdir().unwrap();
    let pkg = root.path().join("pkg");
    std::fs::create_dir(&pkg).unwrap();
    std::fs::write(pkg.join("marker.txt"), "in-pkg").unwrap();

    let beam = Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        dir: Some("pkg".to_string()),
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["cat marker.txt".to_string()],
            executor: None,
        }),
        allow_failure: false,
    };

    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![beam],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }

    assert!(
        events.iter().any(|e| matches!(
            e,
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Success { .. },
            } if name == "build"
        )),
        "beam should succeed"
    );
    assert!(
        output_lines(&events, "build").iter().any(|l| l == "in-pkg"),
        "cat should read pkg/marker.txt relative to dir"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p aurora-core --test scheduler_dir_test dir_sets_command_working_directory`
Expected: FAIL — without the override, the command runs from `root` where `marker.txt` does not exist, so `cat` fails (no `in-pkg` line and the beam does not report success).

- [ ] **Step 3: Override `working_dir` in `run_beam_task`**

In `crates/aurora-core/src/scheduler.rs`, immediately after the `if beam.run.is_none() { ... }` aggregation early-return block (around line 420) and before the executor `match`, add:

```rust
    // `dir` rebases everything the beam does (gates, inputs/outputs, run
    // commands) onto that directory. A relative `dir` joins onto the Beamfile
    // directory; an absolute one replaces it (Path::join semantics).
    let working_dir = match &beam.dir {
        Some(dir) => working_dir.join(dir),
        None => working_dir,
    };
```

This shadows the `working_dir` bound from `TaskEnv`; `gate_skip_reason`, `cache_lookup_blocking`, and the `ExecutionInput` built below all read this shadowed binding.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p aurora-core --test scheduler_dir_test dir_sets_command_working_directory`
Expected: PASS.

- [ ] **Step 5: Add a cache-under-dir test**

Append to `crates/aurora-core/tests/scheduler_dir_test.rs` a test proving relative `inputs` are hashed under `dir` (so caching is package-local):

```rust
/// A relative `input` is resolved under `dir`: changing a file inside `dir`
/// busts the cache, and an unchanged file yields a cache-skip on rerun.
#[tokio::test]
async fn dir_scopes_input_hashing() {
    use aurora_core::scheduler::SkipReason;

    let root = tempfile::tempdir().unwrap();
    let pkg = root.path().join("pkg");
    std::fs::create_dir(&pkg).unwrap();
    std::fs::write(pkg.join("in.txt"), "v1").unwrap();

    let make = || Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec!["in.txt".to_string()],
        outputs: vec!["out.txt".to_string()],
        dir: Some("pkg".to_string()),
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["echo done > out.txt".to_string()],
            executor: None,
        }),
        allow_failure: false,
    };

    // First run populates the cache (input read from pkg/in.txt, output
    // written to pkg/out.txt because cwd == pkg).
    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![make()],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();
    while rx.try_recv().is_ok() {}

    // Second run with the input unchanged: cache hit -> Skipped(Cached).
    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![make()],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();
    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }
    assert!(
        events.iter().any(|e| matches!(
            e,
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Skipped { reason: SkipReason::Cached },
            } if name == "build"
        )),
        "unchanged pkg/in.txt should produce a cache skip"
    );
}
```

- [ ] **Step 6: Run the new tests and clippy**

Run: `cargo test -p aurora-core --test scheduler_dir_test && cargo clippy --workspace -- -D warnings`
Expected: both tests pass, no clippy warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/aurora-core/src/scheduler.rs crates/aurora-core/tests/scheduler_dir_test.rs
git commit -m ":sparkles: feat(scheduler): run beams in their declared dir"
```

---

### Task 4: Fail clearly when `dir` is missing

**Files:**
- Modify: `crates/aurora-core/src/scheduler.rs` (right after the `working_dir` override from Task 3)
- Test: `crates/aurora-core/tests/scheduler_dir_test.rs`

**Interfaces:**
- Consumes: the shadowed `working_dir` (Task 3), the existing `classify_execution(&Result<ExecutionOutput>, allow_failure: bool, duration: Duration) -> (BeamStatus, BeamOutcome)` helper, and the `SchedulerEvent::{BeamOutput, BeamCompleted}` events.
- Produces: a beam with a non-existent `dir` completes as a failure with a clear stderr line, mirroring the unknown-executor path.

- [ ] **Step 1: Write the failing test**

Append to `crates/aurora-core/tests/scheduler_dir_test.rs`:

```rust
/// A beam whose `dir` does not exist fails with a clear error rather than a
/// raw shell "cannot cd" or a confusing cache miss.
#[tokio::test]
async fn missing_dir_fails_beam_clearly() {
    let root = tempfile::tempdir().unwrap();

    let beam = Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        dir: Some("does-not-exist".to_string()),
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["echo hi".to_string()],
            executor: None,
        }),
        allow_failure: false,
    };

    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![beam],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }

    assert!(
        events.iter().any(|e| matches!(
            e,
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Failed { .. },
            } if name == "build"
        )),
        "missing dir should fail the beam"
    );
    assert!(
        output_lines(&events, "build")
            .iter()
            .any(|l| l.contains("working directory does not exist")
                && l.contains("does-not-exist")),
        "error should name the missing directory"
    );
}
```

Note: confirm the failure variant name in `crates/aurora-core/src/scheduler.rs` (`BeamStatus::Failed`) and match its fields with `{ .. }`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p aurora-core --test scheduler_dir_test missing_dir_fails_beam_clearly`
Expected: FAIL — without the check the beam reaches `sh`, which prints its own error and the assertions on the message/variant do not hold.

- [ ] **Step 3: Add the missing-directory check**

In `crates/aurora-core/src/scheduler.rs`, immediately after the `working_dir` override added in Task 3, add:

```rust
    // A declared `dir` that is not an existing directory is a configuration
    // error: fail loudly with the offending path instead of leaking a raw
    // `sh: cannot cd` or a confusing cache miss. Mirrors the unknown-executor
    // failure path.
    if beam.dir.is_some() && !working_dir.is_dir() {
        let err = anyhow::anyhow!("working directory does not exist: {}", working_dir.display());
        let _ = tx
            .send(SchedulerEvent::BeamOutput {
                name: beam.name.clone(),
                line: format!("aurora: {err:#}"),
                is_stderr: true,
            })
            .await;
        let (status, outcome) = classify_execution(&Err(err), beam.allow_failure, Duration::ZERO);
        let _ = tx
            .send(SchedulerEvent::BeamCompleted {
                name: beam.name.clone(),
                status,
            })
            .await;
        return (beam.name, outcome);
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p aurora-core --test scheduler_dir_test missing_dir_fails_beam_clearly`
Expected: PASS.

- [ ] **Step 5: Run the full core suite and clippy**

Run: `cargo test -p aurora-core && cargo clippy --workspace -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/aurora-core/src/scheduler.rs crates/aurora-core/tests/scheduler_dir_test.rs
git commit -m ":sparkles: feat(scheduler): fail clearly on a missing beam dir"
```

---

### Task 5: Documentation — tick the ROADMAP item

**Files:**
- Modify: `ROADMAP.md:40-42`

**Interfaces:** none.

- [ ] **Step 1: Check the ROADMAP item off**

In `ROADMAP.md`, change the per-beam working directory bullet from unchecked to checked and note it shipped:

```markdown
- [x] **Per-beam working directory (`dir`)** — a beam declares `dir = "..."`;
  its run commands, `inputs`/`outputs` and gates resolve against that
  directory. Relative to the Beamfile directory, absolute paths honoured.
```

(Do not touch `Beamfile` or `README.md` in this task — they carry unrelated in-progress local edits.)

- [ ] **Step 2: Verify the whole workspace**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --all --check`
Expected: all tests pass, no clippy warnings, formatting clean.

- [ ] **Step 3: Commit**

```bash
git add ROADMAP.md
git commit -m ":memo: docs: mark per-beam dir shipped in the roadmap"
```

---

## Self-Review

**Spec coverage:**
- DSL `dir` field + `${var.name}` interpolation → Tasks 1, 2.
- "Beam root" semantics (run cwd, inputs/outputs/cache, gates via one `working_dir`) → Task 3.
- Relative-vs-absolute resolution (`Path::join`) → Task 3 (Global Constraints).
- Cache store stays at repo root; env block unaffected → no code change needed (documented as invariants; Task 3 preserves them by only shadowing the per-beam `working_dir`, not the cache path set in `Scheduler::new`).
- Missing-directory clear error → Task 4.
- No `dir`-traversal sandboxing → intentionally no task (spec rationale).
- ROADMAP checkbox → Task 5.

**Placeholder scan:** none — every code and test step carries complete content.

**Type consistency:** `Beam.dir: Option<String>` used identically across Tasks 1-4; `interpolate_command`, `classify_execution`, `SchedulerEvent::{BeamOutput, BeamCompleted}`, and `BeamStatus::{Success, Failed, Skipped}` match the signatures read from the current source. Task 4 Step 1 flags the one name to confirm against source (`BeamStatus::Failed`).
