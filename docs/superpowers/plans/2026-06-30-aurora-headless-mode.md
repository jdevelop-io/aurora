# Aurora headless mode — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `aurora` run beams without the ratatui TUI, streaming plain prefixed output and returning a meaningful exit code, so it is usable in scripts and CI.

**Architecture:** The scheduler already emits `SchedulerEvent`s over an `mpsc` channel and the TUI is just one consumer. We add a second consumer — a pure text renderer in a new `headless` module — and route to it in `main.rs` based on auto-detection (`stdout().is_terminal()`) plus `-i`/`--no-tui` overrides. `aurora-core` is untouched.

**Tech Stack:** Rust 2021, tokio, clap 4, `std::io::IsTerminal`. No new external crates (ANSI colour via raw escape codes).

## Global Constraints

- Minimum Rust toolchain: **1.70** (for `std::io::IsTerminal`). Copy verbatim into any toolchain note.
- Edition: **2021** (inherited from `[workspace.package]`).
- **No new external dependency.** Colour is raw ANSI; terminal detection is std.
- Status markers are **ASCII**: `[OK]`, `[FAIL]`, `[SKIP]`, `[WARN]`, `[CANC]`. Unicode stays in the TUI only.
- Inline code comments and git commit messages: **French** (repo convention). User-facing docs (`README.md`, `docs/`, plugin docs, CLI help): **English**.
- Commits: gitmoji + Conventional Commits. No Claude/Anthropic attribution.
- Tests live in each crate's `tests/` directory as integration tests, not `#[cfg(test)]` inline modules (repo convention). To make the renderer testable this way, the `aurora` package gains a small library target exposing `headless`.
- Spec deviation (intentional): the spec's optional `(<total>s)` wall-clock suffix on the summary line is **omitted** in v1 to keep the renderer pure and its tests deterministic. Per-beam durations remain in the recap.

---

### Task 1: Headless text renderer

Adds a library target to the `aurora` package and the pure `run_headless` renderer that drains a `SchedulerEvent` stream into two injected writers and returns overall success. Fully unit-tested in isolation; `main.rs` is not touched in this task.

**Files:**
- Create: `crates/aurora/src/lib.rs`
- Create: `crates/aurora/src/headless.rs`
- Test: `crates/aurora/tests/headless_test.rs`

**Interfaces:**
- Consumes: `aurora_core::scheduler::{SchedulerEvent, BeamStatus, SkipReason}` (already public, unchanged).
- Produces:
  ```rust
  // crates/aurora/src/headless.rs
  pub async fn run_headless(
      beam_names: &[String],            // for prefix-width alignment
      use_color: bool,                  // ANSI on markers/prefix when true
      rx: tokio::sync::mpsc::Receiver<aurora_core::scheduler::SchedulerEvent>,
      out: &mut impl std::io::Write,    // stdout in production
      err: &mut impl std::io::Write,    // stderr in production
  ) -> std::io::Result<bool>            // overall success (drives exit code)
  ```

- [ ] **Step 1: Write the failing test**

Create `crates/aurora/tests/headless_test.rs`:

```rust
use aurora::headless::run_headless;
use aurora_core::scheduler::{BeamStatus, SchedulerEvent, SkipReason};
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn streams_prefixed_output_routes_stderr_and_builds_recap() {
    let (tx, rx) = mpsc::channel(16);
    let beams = vec!["build".to_string(), "test".to_string()]; // width = 5

    tx.send(SchedulerEvent::BeamStarted { name: "build".into() }).await.unwrap();
    tx.send(SchedulerEvent::BeamOutput { name: "build".into(), line: "compiling".into(), is_stderr: false }).await.unwrap();
    tx.send(SchedulerEvent::BeamOutput { name: "test".into(), line: "boom".into(), is_stderr: true }).await.unwrap();
    tx.send(SchedulerEvent::BeamCompleted { name: "build".into(), status: BeamStatus::Success { duration: Duration::from_millis(4200), cached: false } }).await.unwrap();
    tx.send(SchedulerEvent::BeamCompleted { name: "test".into(), status: BeamStatus::Failed { exit_code: 1, duration: Duration::from_millis(1800) } }).await.unwrap();
    tx.send(SchedulerEvent::AllDone { success: false }).await.unwrap();
    drop(tx);

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let success = run_headless(&beams, false, rx, &mut out, &mut err).await.unwrap();
    let out = String::from_utf8(out).unwrap();
    let err = String::from_utf8(err).unwrap();

    assert!(!success);
    assert!(out.contains("[build] compiling"), "stdout prefix:\n{out}");
    // "test" is padded to the width of "build" (5) → "[test ]"
    assert!(err.contains("[test ] boom"), "stderr prefix/padding:\n{err}");
    assert!(!out.contains("boom"), "stderr line must not leak to stdout");
    assert!(out.contains("[OK]"), "recap ok marker:\n{out}");
    assert!(out.contains("4.2s"), "recap duration:\n{out}");
    assert!(out.contains("[FAIL]"), "recap fail marker:\n{out}");
    assert!(out.contains("exit 1"), "recap exit code:\n{out}");
    assert!(out.contains("Done: 1 ok, 1 failed"), "summary:\n{out}");
}

#[tokio::test]
async fn allow_failure_counts_as_ok_and_overall_can_be_true() {
    let (tx, rx) = mpsc::channel(16);
    let beams = vec!["deploy".to_string()];

    tx.send(SchedulerEvent::BeamCompleted { name: "deploy".into(), status: BeamStatus::FailedAllowed { exit_code: 2, duration: Duration::from_millis(300) } }).await.unwrap();
    tx.send(SchedulerEvent::AllDone { success: true }).await.unwrap();
    drop(tx);

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let success = run_headless(&beams, false, rx, &mut out, &mut err).await.unwrap();
    let out = String::from_utf8(out).unwrap();

    assert!(success);
    assert!(out.contains("[WARN]"), "warn marker:\n{out}");
    assert!(out.contains("(allowed)"), "allowed note:\n{out}");
    assert!(out.contains("Done: 1 ok, 0 failed"), "summary:\n{out}");
}

#[tokio::test]
async fn skipped_and_cancelled_markers_and_color_toggle() {
    let (tx, rx) = mpsc::channel(16);
    let beams = vec!["lint".to_string()];

    tx.send(SchedulerEvent::BeamCompleted { name: "lint".into(), status: BeamStatus::Skipped { reason: SkipReason::Cached } }).await.unwrap();
    tx.send(SchedulerEvent::AllDone { success: true }).await.unwrap();
    drop(tx);

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    run_headless(&beams, true, rx, &mut out, &mut err).await.unwrap();
    let out = String::from_utf8(out).unwrap();

    assert!(out.contains("[SKIP]"), "skip marker:\n{out}");
    assert!(out.contains("cached"), "skip reason:\n{out}");
    // use_color = true wraps markers in ANSI
    assert!(out.contains("\u{1b}["), "ansi escape present:\n{out:?}");
    assert!(out.contains("\u{1b}[0m"), "ansi reset present:\n{out:?}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p aurora --test headless_test`
Expected: FAIL — compilation error, `use of undeclared crate or module aurora::headless` (no library target yet).

- [ ] **Step 3: Create the library target**

Create `crates/aurora/src/lib.rs`:

```rust
//! Bibliothèque interne du binaire `aurora` : expose les composants
//! testables indépendamment de la TUI (mode headless).

pub mod headless;
```

- [ ] **Step 4: Implement the renderer**

Create `crates/aurora/src/headless.rs`:

```rust
//! Rendu texte du mode headless : draine le flux d'événements du scheduler,
//! l'affiche en lignes préfixées par beam (stdout/stderr séparés), puis imprime
//! un récap final. Renvoie le succès global, qui pilote le code de sortie.

use std::io::Write;

use aurora_core::scheduler::{BeamStatus, SchedulerEvent, SkipReason};
use tokio::sync::mpsc;

/// Enrobe `text` dans un code couleur ANSI lorsque `use_color` est vrai.
fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    } else {
        text.to_string()
    }
}

/// Formate une durée en secondes avec une décimale (ex. "4.2s").
fn fmt_duration(d: std::time::Duration) -> String {
    format!("{:.1}s", d.as_secs_f64())
}

/// Construit la ligne de récap d'un beam terminé.
/// Renvoie `None` pour les statuts non terminaux (Pending/Running), jamais émis ici.
fn recap_line(name: &str, status: &BeamStatus, width: usize, use_color: bool) -> Option<String> {
    let (marker, color, detail) = match status {
        BeamStatus::Success { duration, cached: false } => ("OK", "32", fmt_duration(*duration)),
        BeamStatus::Success { cached: true, .. } => ("OK", "32", "cached".to_string()),
        BeamStatus::Skipped { reason } => {
            let r = match reason {
                SkipReason::Cached => "cached",
                SkipReason::ConditionFalse => "condition false",
            };
            ("SKIP", "33", r.to_string())
        }
        BeamStatus::Failed { exit_code, duration } => {
            ("FAIL", "31", format!("exit {exit_code} {}", fmt_duration(*duration)))
        }
        BeamStatus::FailedAllowed { exit_code, duration } => {
            ("WARN", "33", format!("exit {exit_code} (allowed) {}", fmt_duration(*duration)))
        }
        BeamStatus::Cancelled => ("CANC", "35", "cancelled".to_string()),
        BeamStatus::Pending | BeamStatus::Running => return None,
    };
    // Marqueur cadré sur 6 caractères ("[FAIL]") avant coloration, pour aligner
    // sans que les codes ANSI (largeur nulle) ne décalent les colonnes.
    let marker = format!("{:<6}", format!("[{marker}]"));
    let marker = paint(&marker, color, use_color);
    Some(format!("{marker} {name:<width$}  {detail}"))
}

/// Draine le flux d'événements jusqu'à `AllDone`, imprime les lignes préfixées
/// puis le récap. Renvoie le succès global porté par `AllDone`.
pub async fn run_headless(
    beam_names: &[String],
    use_color: bool,
    mut rx: mpsc::Receiver<SchedulerEvent>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<bool> {
    let width = beam_names.iter().map(|n| n.len()).max().unwrap_or(0);
    let mut recap: Vec<(String, BeamStatus)> = Vec::new();
    let mut overall = true;

    while let Some(event) = rx.recv().await {
        match event {
            SchedulerEvent::BeamOutput { name, line, is_stderr } => {
                let prefix = paint(&format!("[{name:<width$}]"), "90", use_color);
                if is_stderr {
                    writeln!(err, "{prefix} {line}")?;
                } else {
                    writeln!(out, "{prefix} {line}")?;
                }
            }
            SchedulerEvent::BeamCompleted { name, status } => recap.push((name, status)),
            SchedulerEvent::BeamStarted { .. } => {}
            SchedulerEvent::AllDone { success } => {
                overall = success;
                break;
            }
        }
    }

    writeln!(out)?;
    let mut ok = 0usize;
    let mut failed = 0usize;
    for (name, status) in &recap {
        if let Some(line) = recap_line(name, status, width, use_color) {
            writeln!(out, "{line}")?;
        }
        match status {
            BeamStatus::Success { .. }
            | BeamStatus::Skipped { .. }
            | BeamStatus::FailedAllowed { .. } => ok += 1,
            BeamStatus::Failed { .. } | BeamStatus::Cancelled => failed += 1,
            BeamStatus::Pending | BeamStatus::Running => {}
        }
    }
    writeln!(out, "Done: {ok} ok, {failed} failed")?;

    Ok(overall)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p aurora --test headless_test`
Expected: PASS (3 tests).

- [ ] **Step 6: Lint and format**

Run: `cargo fmt --all && cargo clippy -p aurora -- -D warnings`
Expected: no warnings, no diff.

- [ ] **Step 7: Commit**

```bash
git add crates/aurora/src/lib.rs crates/aurora/src/headless.rs crates/aurora/tests/headless_test.rs
git commit -m ":sparkles: feat(headless): ajouter le renderer texte non interactif"
```

---

### Task 2: CLI routing, auto-detection and exit code

Adds the `--no-tui` and `-i`/`--interactive` flags, computes the interactive/headless decision, and wires the headless path in `main.rs`: resolve the target through config `default`, spawn the scheduler, drain the events with `run_headless`, and `exit(1)` on failure.

**Files:**
- Modify: `crates/aurora/src/main.rs`
- Test: `crates/aurora/tests/headless_cli_test.rs`

**Interfaces:**
- Consumes: `aurora::headless::run_headless` (Task 1), `aurora_core::scheduler::Scheduler::{new, run, run_cancellable}` (unchanged), `resolve_target` (existing in `main.rs`).
- Produces: CLI behaviour — `aurora <beam> --no-tui` streams plain output and exits `0` on success, `1` on failure; auto-detection selects headless when stdout is not a TTY.

- [ ] **Step 1: Write the failing CLI test**

Create `crates/aurora/tests/headless_cli_test.rs`:

```rust
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
    assert!(output.status.success(), "exit status: {:?}\nstdout:\n{stdout}", output.status.code());
    assert!(stdout.contains("hello"), "command output streamed:\n{stdout}");
    assert!(stdout.contains("[ok"), "per-beam prefix present:\n{stdout}");
    assert!(stdout.contains("[OK]"), "recap ok marker:\n{stdout}");
    let _ = fs::remove_dir_all(&dir);
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
    assert_eq!(output.status.code(), Some(1), "expected exit 1\nstdout:\n{stdout}");
    assert!(stdout.contains("[FAIL]"), "recap fail marker:\n{stdout}");
    let _ = fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p aurora --test headless_cli_test`
Expected: FAIL — `--no-tui` is an unknown argument, so clap exits with code 2 and the binary never runs headless. (`passing_beam...` fails the `status.success()` assertion; `failing_beam...` fails the `Some(1)` assertion.)

- [ ] **Step 3: Add the two flags to the clap command**

In `crates/aurora/src/main.rs`, in the `Command::new("aurora")` builder, add the two arguments immediately after the existing `var` argument (currently ending around line 24):

```rust
        .arg(Arg::new("var").long("var").action(clap::ArgAction::Append)
             .help("Override variable: --var key=value"))
        .arg(Arg::new("no-tui").long("no-tui").action(clap::ArgAction::SetTrue)
             .help("Force plain output, even in a terminal"))
        .arg(Arg::new("interactive").long("interactive").short('i')
             .action(clap::ArgAction::SetTrue)
             .conflicts_with("no-tui")
             .help("Force the TUI, even when output is not a terminal"));
```

- [ ] **Step 4: Add the library import and the interactive decision**

At the top of `crates/aurora/src/main.rs`, after the existing `use` lines, add:

```rust
use aurora::headless;
use std::io::IsTerminal;
```

Then replace the current target-resolution block (the `let target = if let Some(beam_name) ... else { return Ok(()); };` spanning roughly lines 57-82) with:

```rust
    let interactive = matches.get_flag("interactive")
        || (std::io::stdout().is_terminal() && !matches.get_flag("no-tui"));

    // Résolution de la cible : picker en interactif, beam `default` en headless
    // (le picker est intrinsèquement interactif et n'existe pas hors TTY).
    let target = if interactive {
        if let Some(beam_name) = matches.get_one::<String>("beam") {
            beam_name.clone()
        } else if let Some(picker_results) = aurora_tui::run_picker(
            beam_file.beams.iter().map(|b| (b.name.clone(), b.description.clone(), b.depends_on.clone())).collect()
        )? {
            if picker_results.len() == 1 {
                picker_results.into_iter().next().unwrap()
            } else {
                // Multi-select : beam virtuel __multi__ dépendant des beams sélectionnés
                let virtual_beam = aurora_core::ast::Beam {
                    name: "__multi__".to_string(),
                    description: Some("Multi-beam run".to_string()),
                    depends_on: picker_results,
                    inputs: vec![],
                    outputs: vec![],
                    skip_if: None,
                    condition: None,
                    run: None,
                    allow_failure: false,
                };
                beam_file.beams.push(virtual_beam);
                "__multi__".to_string()
            }
        } else {
            return Ok(());
        }
    } else {
        resolve_target(&beam_file, matches.get_one::<String>("beam").map(|s| s.as_str()))?
    };
```

- [ ] **Step 5: Branch the consumer (TUI vs headless)**

The executor/env setup (building `executors`, `working_dir`, `env`) stays unchanged. Replace the channel/scheduler/TUI block (currently roughly lines 97-147, from `let (tx, rx) = mpsc::channel(128);` through the `run_execution_tui(...)` call) with:

```rust
    let (tx, rx) = mpsc::channel(128);
    // Exclure le beam virtuel __multi__ de la liste affichée / des préfixes
    let beam_info: Vec<(String, Vec<String>)> = beam_file.beams.iter()
        .filter(|b| b.name != "__multi__")
        .map(|b| (b.name.clone(), b.depends_on.clone()))
        .collect();

    let beams = beam_file.beams.clone();
    let scheduler = Scheduler::new(
        beams,
        executors.clone(),
        tx,
        beam_file.config.as_ref().and_then(|c| c.max_parallelism),
        working_dir.clone(),
        env.clone(),
    );

    if interactive {
        let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
        let target_clone = target.clone();
        tokio::spawn(async move {
            if let Err(e) = scheduler.run_cancellable(&target_clone, &[], cancel_rx).await {
                eprintln!("Scheduler error: {}", e);
            }
        });

        let rerun_beams: Vec<_> = beam_file.beams.iter().filter(|b| b.name != "__multi__").cloned().collect();
        let rerun_executors = executors.clone();
        let rerun_max_par = beam_file.config.as_ref().and_then(|c| c.max_parallelism);
        let rerun_working_dir = working_dir.clone();
        let rerun_env = env.clone();

        let rerun = move |root: String, pre_success: Vec<String>| -> (mpsc::Receiver<SchedulerEvent>, mpsc::UnboundedSender<String>) {
            let (tx, rx) = mpsc::channel(128);
            let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
            let scheduler = Scheduler::new(
                rerun_beams.clone(),
                rerun_executors.clone(),
                tx,
                rerun_max_par,
                rerun_working_dir.clone(),
                rerun_env.clone(),
            );
            tokio::runtime::Handle::current().spawn(async move {
                if let Err(e) = scheduler.run_cancellable(&root, &pre_success, cancel_rx).await {
                    eprintln!("Scheduler error: {}", e);
                }
            });
            (rx, cancel_tx)
        };

        aurora_tui::run_execution_tui(beam_info, rx, cancel_tx, rerun).await?;
    } else {
        // Mode headless : pas d'annulation interactive, `run` gère son propre canal.
        let target_clone = target.clone();
        tokio::spawn(async move {
            if let Err(e) = scheduler.run(&target_clone, &[]).await {
                eprintln!("Scheduler error: {}", e);
            }
        });

        let beam_names: Vec<String> = beam_info.iter().map(|(name, _)| name.clone()).collect();
        let use_color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let mut stdout = std::io::stdout();
        let mut stderr = std::io::stderr();
        let success = headless::run_headless(&beam_names, use_color, rx, &mut stdout, &mut stderr).await?;
        if !success {
            std::process::exit(1);
        }
    }

    Ok(())
```

Note: `run` (unlike `run_cancellable`) creates and immediately drops its own cancel channel internally (see `Scheduler::run`), so the headless path needs no cancel wiring. The `tokio::select!` cancel branch uses a `Some(name) = cancel_rx.recv()` pattern, so a closed cancel channel simply disables that branch — no spin.

- [ ] **Step 6: Build, then run the CLI test to verify it passes**

Run: `cargo build -p aurora && cargo test -p aurora --test headless_cli_test`
Expected: PASS (2 tests). `aurora ok --no-tui` exits 0 with `hello` and `[OK]`; `aurora boom --no-tui` exits 1 with `[FAIL]`.

- [ ] **Step 7: Run the full workspace suite, lint and format**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --all --check`
Expected: all green, no warnings, no formatting diff.

- [ ] **Step 8: Manual smoke test of auto-detection**

Run: `cargo run -p aurora -- ok --no-tui` then `cargo run -p aurora -- ok | cat` from a directory containing the fixture Beamfile (or the repo root using a real beam such as `fmt`).
Expected: both produce plain prefixed output with a recap (the pipe case proves auto-detection routes to headless without any flag). `echo $?` reflects success/failure.

- [ ] **Step 9: Commit**

```bash
git add crates/aurora/src/main.rs crates/aurora/tests/headless_cli_test.rs
git commit -m ":sparkles: feat(cli): router vers le mode headless et propager le code de sortie"
```

---

### Task 3: Documentation and plugin update (Definition of Done)

Documents the headless mode everywhere the spec requires. No automated test; verification is review of the rendered text.

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md` (root)
- Modify: `claude-code-plugin/skills/using-aurora/references/cli.md`
- Modify: `claude-code-plugin/skills/using-aurora/SKILL.md`
- Modify: `claude-code-plugin/agents/aurora-expert.md`
- Modify: `claude-code-plugin/hooks/session-context.sh`

- [ ] **Step 1: README.md — add a headless section**

Read `README.md`, find the CLI/usage section, and add this subsection (English):

```markdown
### Non-interactive (headless) mode

Aurora auto-detects whether to show the TUI: when standard output is a terminal
it opens the ratatui interface; when output is piped or redirected (scripts, CI)
it runs headless, streaming plain per-beam output and printing a final recap.

Two flags force the choice explicitly:

- `--no-tui` — force plain output, even in a terminal.
- `-i`, `--interactive` — force the TUI, even when output is not a terminal.

Headless output prefixes each line with its beam name (stdout and stderr are
kept separate) and ends with an ASCII recap:

```
[build] Compiling aurora-core v0.5.0
[test]  running 12 tests

[OK]   build  4.2s
[FAIL] test   exit 1 1.8s
Done: 1 ok, 1 failed
```

Exit code: `0` when every beam succeeds (beams marked `allow_failure` count as
success), `1` when any beam fails. In headless mode the target beam is taken
from the `aurora { default = ... }` block when no beam is given; the interactive
picker is only available with a TTY or `-i`.
```

- [ ] **Step 2: CLAUDE.md (root) — correct the misleading TUI sentence**

Read `CLAUDE.md`, find the line:

> Run the built tool against a Beamfile: `aurora`, `aurora <beam>`, `aurora --list`, `aurora --dry-run`, `aurora --no-cache`, `aurora --var key=val`. With no beam argument and a TTY, the picker TUI opens.

Replace the trailing sentence so it reads:

```markdown
Run the built tool against a Beamfile: `aurora`, `aurora <beam>`, `aurora --list`, `aurora --dry-run`,
`aurora --no-cache`, `aurora --var key=val`, `aurora --no-tui`, `aurora -i`. Output mode is auto-detected:
with a TTY the execution TUI runs (and, when no beam is given, the picker opens first); when output is not a
TTY, or with `--no-tui`, Aurora runs headless (plain prefixed logs, ASCII recap, exit `1` on any beam failure).
`-i`/`--interactive` forces the TUI; in headless mode the target comes from the `default` beam since there is
no picker.
```

- [ ] **Step 3: cli.md — document the flags, auto-detection and exit codes**

Read `claude-code-plugin/skills/using-aurora/references/cli.md`. Under the `## Flags` list, add:

```markdown
- `--no-tui` — force plain, non-interactive output even in a terminal. Output is streamed per beam (lines
  prefixed with the beam name, stdout and stderr kept separate) and ends with an ASCII recap
  (`[OK]`/`[FAIL]`/`[SKIP]`/`[WARN]`/`[CANC]`) plus a `Done: N ok, M failed` summary.
- `-i`, `--interactive` — force the TUI even when output is not a terminal. Mutually exclusive with `--no-tui`.
```

Then update the positional-argument note: in headless mode (no TTY, or `--no-tui`), the `default` beam from the
`aurora {}` block IS used to run when no beam is given (not only by `--dry-run`); the picker only opens with a TTY
or `-i`. Add a `## Output mode and exit codes` section:

```markdown
## Output mode and exit codes

Aurora auto-detects the output mode via `stdout().is_terminal()`: a TTY gets the TUI, a pipe/redirect gets
headless. `--no-tui` and `-i` override this. Headless exit codes: `0` if all beams succeed (`allow_failure`
beams count as success), `1` if any beam fails. This makes Aurora usable as a CI step:

​```bash
aurora test --no-tui   # plain logs, exit 1 on failure
aurora build | tee build.log   # auto-headless because stdout is piped
​```
```

- [ ] **Step 4: SKILL.md and aurora-expert.md — mention headless**

Read `claude-code-plugin/skills/using-aurora/SKILL.md` and `claude-code-plugin/agents/aurora-expert.md`. Wherever they describe running beams / the TUI, add one sentence:

```markdown
Aurora auto-detects the output mode: a TTY shows the ratatui TUI; a pipe or CI runs headless with plain
prefixed logs and a meaningful exit code (`--no-tui` forces headless, `-i` forces the TUI).
```

- [ ] **Step 5: session-context.sh — update execution description if present**

Read `claude-code-plugin/hooks/session-context.sh`. If it describes how beams are executed or mentions the TUI/interactivity, update that text to note the auto-detected headless mode and the `--no-tui`/`-i` flags. If it does not describe execution behaviour, leave it unchanged and record that in the commit body.

- [ ] **Step 6: Verify the docs build/render and re-run the plugin linter**

Run: `shellcheck claude-code-plugin/hooks/session-context.sh` (the repo lints plugin hooks with shellcheck).
Expected: no new warnings. Visually re-read each edited Markdown block for correct fenced code blocks.

- [ ] **Step 7: Commit**

```bash
git add README.md CLAUDE.md claude-code-plugin/
git commit -m ":memo: docs(headless): documenter le mode non interactif et mettre à jour le plugin"
```

---

## Self-Review

**Spec coverage:**
- CLI surface (`--no-tui`, `-i`, auto-detection, `conflicts_with`) → Task 2, Steps 3-4.
- Component placement (`crates/aurora/src/headless.rs`, not in `aurora-tui`) → Task 1.
- Event stream consumed unchanged → Task 1 (`run_headless` match arms).
- Renderer behaviour (prefix + alignment, stdout/stderr routing, verbatim command output, ASCII recap markers, colour gated on TTY + `NO_COLOR`) → Task 1, Steps 4; colour caller-side gating in Task 2, Step 5.
- Exit code (`0`/`1`, `process::exit(1)`) → Task 2, Step 5; tested in `failing_beam_exits_one`.
- Signals/cancellation v1 (no handler, `run` self-manages cancel channel, `kill_on_drop`) → Task 2, Step 5 note.
- Testing seam (injected writers, synthetic events) → Task 1 tests; end-to-end exit codes → Task 2 tests.
- Docs & plugin DoD (README, CLAUDE.md, cli.md, SKILL.md, aurora-expert.md, session-context.sh) → Task 3.
- Resolved decisions (`--no-tui` naming, Option B, streamed format, ASCII markers) → reflected throughout.
- Intentional deviation: spec summary `(<total>s)` omitted → recorded in Global Constraints.

**Placeholder scan:** No TBD/TODO; every code step contains complete code; the only "read the file then edit" steps are documentation (Task 3) where the inserted text is given verbatim and only the anchor must be located.

**Type consistency:** `run_headless` signature is identical in the Interfaces block, Task 1 implementation, and both test call sites. `SchedulerEvent`/`BeamStatus`/`SkipReason` variant names and fields match the source (`Success { duration, cached }`, `Failed { exit_code, duration }`, `FailedAllowed { exit_code, duration }`, `Skipped { reason }`, `Cancelled`; `SkipReason::{Cached, ConditionFalse}`). `Scheduler::{new, run, run_cancellable}` signatures match `main.rs` usage.
