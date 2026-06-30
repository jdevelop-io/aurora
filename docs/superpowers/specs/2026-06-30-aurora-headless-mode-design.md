# Aurora headless (non-interactive) execution mode — design

Date: 2026-06-30
Status: approved (pending spec review)

## Goal

Let Aurora run beams without the ratatui TUI, so it is usable from scripts, CI
pipelines, and pipes. Today every real execution ends in
`aurora_tui::run_execution_tui(...)`, regardless of whether stdout is a
terminal, and the process never propagates a non-zero exit code on beam
failure. This makes Aurora unsuitable for non-interactive use, which is a
central use case for a task runner.

The headless mode streams beam output as plain prefixed text, prints a final
status recap, and exits with a meaningful status code.

## Non-goals

- No interactive beam picker in headless mode. A pick step is inherently
  interactive; headless resolves the target through configuration instead.
- No rerun (`r` key) in headless mode. Rerun is a TUI affordance.
- No graceful cancellation handler in v1. `Ctrl-C` terminates the process and
  child processes die via `kill_on_drop` (already in place). A cooperative
  `tokio::signal::ctrl_c` handler that emits a `cancelled` recap is a future
  improvement.
- No change to `aurora-core`. The scheduler already emits everything needed.

## Design philosophy

Headless is the engine; the TUI is an optional front-end that auto-activates
for humans. The scheduler is already decoupled from the TUI through an
`mpsc::Receiver<SchedulerEvent>`; the TUI is merely one consumer. The headless
mode is a second consumer of the same event stream. The core stays untouched.

## CLI surface (auto-detection plus explicit overrides)

The default behaviour is automatic, following the `git` / `cargo` /
`ls --color=auto` convention: a TUI for humans, plain text for everything else.
Two flags force either direction.

```rust
.arg(Arg::new("no-tui").long("no-tui").action(clap::ArgAction::SetTrue)
     .help("Force plain output, even in a terminal"))
.arg(Arg::new("interactive").long("interactive").short('i')
     .action(clap::ArgAction::SetTrue)
     .conflicts_with("no-tui")
     .help("Force the TUI, even when output is not a terminal"))
```

Routing decision in `main.rs`:

```rust
use std::io::IsTerminal;

let interactive = matches.get_flag("interactive")
    || (std::io::stdout().is_terminal() && !matches.get_flag("no-tui"));
```

- `interactive == true` → existing behaviour: open the picker when no beam is
  given, then `run_execution_tui`.
- `interactive == false` → headless path: resolve the target through the
  existing `resolve_target` (which honours the `default` beam from the
  `aurora {}` block), then drain the scheduler event stream with the headless
  renderer. There is no picker in headless mode; a bare `aurora` with no
  configured `default` is a usage error (handled by `resolve_target`, which
  already `bail!`s).

`-i` and `--no-tui` are mutually exclusive (`conflicts_with`), so the intent is
never ambiguous.

Detection reads `stdout`, by convention: if stdout is piped or redirected, the
output is being captured and headless is correct even when stderr is still a
terminal.

## Component placement

A new file `crates/aurora/src/headless.rs` in the binary crate. It does not
belong in `aurora-tui`, whose role is the ratatui interface. The headless
renderer is a CLI presentation concern, wired in the composition root alongside
the TUI path.

```rust
pub async fn run_headless(
    beam_names: &[String],            // for prefix-width alignment
    mut rx: mpsc::Receiver<SchedulerEvent>,
    out: &mut impl std::io::Write,    // injected (stdout in production)
    err: &mut impl std::io::Write,    // injected (stderr in production)
) -> std::io::Result<bool>            // overall success, drives the exit code
```

Injecting the two writers is the testability seam (see Testing).

## Event stream consumed

The scheduler already emits (unchanged):

```rust
enum SchedulerEvent {
    BeamStarted   { name: String },
    BeamCompleted { name: String, status: BeamStatus },
    BeamOutput    { name: String, line: String, is_stderr: bool },
    AllDone       { success: bool },
}

enum BeamStatus {
    Pending,
    Running,
    Success      { duration: Duration, cached: bool },
    Skipped      { reason: SkipReason },
    Failed       { exit_code: i32, duration: Duration },
    FailedAllowed{ exit_code: i32, duration: Duration },
    Cancelled,
}
```

## Renderer behaviour

Output format: streamed, prefixed per beam, parallelism visible (beams
interleave as they run).

- `BeamOutput` → one line, prefixed with `[<beam>] ` (beam name left-padded to
  the longest declared beam name for alignment). Routed to `out` when
  `is_stderr == false`, to `err` when `is_stderr == true`, so
  `aurora build 2>err.log` keeps the correct stream separation. The command
  output itself is passed through verbatim (Unicode, accents, and emojis from a
  beam's command are never transformed).
- `BeamStarted` / `BeamCompleted` are not printed line-by-line during the run
  (the prefixed output already shows progress); `BeamCompleted` results are
  collected for the final recap.
- Final recap, one line per beam, ordered by completion. Status markers are
  **ASCII** (safe on any console, including legacy Windows code pages, and
  greppable):

  ```
  [OK]   build   4.2s
  [FAIL] test    exit 1   1.8s
  [SKIP] lint    cached
  [SKIP] fmt     <skip reason>
  [CANC] deploy  cancelled
  ```

  Marker mapping:
  - `Success { cached: false }` → `[OK]` plus duration.
  - `Success { cached: true }` → `[OK]` plus `cached`.
  - `Skipped { reason }` → `[SKIP]` plus the reason.
  - `Failed { exit_code }` → `[FAIL]` plus `exit <code>` plus duration.
  - `FailedAllowed { exit_code }` → `[WARN]` plus `exit <code> (allowed)` plus
    duration (counts as success for the overall status).
  - `Cancelled` → `[CANC]`.
- Summary line: `Done: <ok> ok, <failed> failed (<total>s)`.
- ASCII markers are confined to the headless recap. The TUI keeps its Unicode.

Colour: ANSI on the prefix and the status markers only when the target stream
is a terminal and `NO_COLOR` is unset; plain otherwise. No new dependency (a
small ANSI helper). When in doubt, no colour.

## Exit code

The renderer drains until `AllDone { success }`:

- `success == true` → exit `0`.
- `success == false` (at least one non-`allow_failure` beam `Failed`) →
  `std::process::exit(1)`.
- Usage and parse errors (no `Beamfile`, no beam and no `default`) are already
  surfaced by `anyhow` with a non-zero exit.

This closes the current gap: the TUI propagates no failure status at all.

## Signals and cancellation

V1 is minimal: no custom handler. `Ctrl-C` (SIGINT) terminates the process; the
scheduler task's executor futures are dropped and child processes are killed
through `kill_on_drop`, which executors already set. `run_cancellable` still
receives a cancel channel; in headless the sender is simply never used.

A cooperative `tokio::signal::ctrl_c` handler that sends a cancellation and lets
the recap report `cancelled` beams is deferred (YAGNI for v1).

## Testing

The injected writers make the renderer unit-testable without spawning
processes. A test in `crates/aurora/tests/` pushes a synthetic sequence of
`SchedulerEvent`s through an `mpsc` channel and asserts on:

- prefixing and left-pad alignment,
- stdout vs stderr routing (`is_stderr`),
- recap content and ASCII markers for each `BeamStatus` variant,
- the summary line,
- the returned overall-success boolean.

The existing `aurora-core` tests already cover scheduling; no core test
changes.

## Definition of Done — documentation and plugin

These are part of the deliverable, not a follow-up:

- `README.md` — CLI section: document `--no-tui`, `-i`/`--interactive`, the
  auto-detection rule, and the exit codes, with a CI example.
- `CLAUDE.md` (root) — correct the misleading line "With no beam argument and a
  TTY, the picker TUI opens": distinguish the picker (no beam) from the
  execution TUI (always, today) and describe the new headless mode and
  auto-detection.
- `claude-code-plugin/skills/using-aurora/references/cli.md` — add the new
  flags, the auto-detection rule, exit codes, and CI examples.
- `claude-code-plugin/skills/using-aurora/SKILL.md` and
  `claude-code-plugin/agents/aurora-expert.md` — mention the headless mode.
- `claude-code-plugin/hooks/session-context.sh` — update if it describes the
  execution behaviour.

## Open decisions resolved

- Override flag is `--no-tui` (names the TUI precisely), not `--no-tty` (a TTY
  is a device, not something a flag disables).
- Headless is the baseline; the TUI auto-activates for humans (Option B), with
  `-i` and `--no-tui` as the two explicit overrides.
- Default headless output is the streamed, per-beam-prefixed format.
- Status markers are ASCII (`[OK]`, `[FAIL]`, `[SKIP]`, `[WARN]`, `[CANC]`).
