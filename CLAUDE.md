# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Aurora is

Aurora is a task runner and build tool written in Rust, an alternative to `make`/`just`/`taskfile`. Tasks are called *
*beams** and are declared in a `Beamfile` (HCL-inspired DSL). Aurora resolves beam dependencies into a directed acyclic
graph (DAG), runs independent beams in parallel on a tokio task pool, caches results by hashing inputs, and ships a
ratatui TUI to follow execution in real time.

## Commands

Aurora dogfoods itself: the repo's own `Beamfile` defines `fmt`, `clippy`, `test`, `build`, `check`. Use Cargo directly
for development:

```bash
cargo build --release            # build the aurora binary -> target/release/aurora
cargo fmt --all                  # format
cargo clippy --workspace -- -D warnings   # lint (warnings are errors)
cargo test --workspace           # run all tests across the workspace
```

Run a single crate's tests, or a single test by name:

```bash
cargo test -p aurora-core                      # one crate
cargo test -p aurora-core scheduler            # tests in an integration test file (tests/scheduler_test.rs)
cargo test -p aurora-tui log_search            # filter by substring
```

Tests live in each crate's `tests/` directory as integration tests (not `#[cfg(test)]` inline modules), so they exercise
the public crate API.

Run the built tool against a Beamfile: `aurora`, `aurora <beam>`, `aurora --list`, `aurora --dry-run`,
`aurora --no-cache`, `aurora --var key=val`, `aurora --no-tui`, `aurora -i`. Output mode is auto-detected:
with a TTY the execution TUI runs (and, when no beam is given, the picker opens first); when output is not a
TTY, or with `--no-tui`, Aurora runs headless (plain prefixed logs, ASCII recap, exit `1` on any beam failure
or on a malformed Beamfile such as a dependency cycle or an unknown dependency).
`-i`/`--interactive` forces the TUI; in headless mode the target comes from the `default` beam since there is
no picker.

## Releasing

The version lives once in the root `Cargo.toml` under `[workspace.package] version`; every crate inherits it via
`version.workspace = true`. Pushing a `v*` git tag triggers `.github/workflows/release.yml`, which cross-builds binaries
for Linux/macOS/Windows and publishes a GitHub release with SHA-256 sums. Use the `release` skill (or
`/release <X.Y.Z|patch|minor|major>` command) to bump, validate, commit, tag, and push, rather than doing it by hand.

## Architecture

Cargo workspace with a strict dependency direction. The `aurora-executor-api` crate sits at the bottom (defines the
`Executor` trait and `ExecutionInput`/`ExecutionOutput`); `aurora-core` and every executor depend on it; the `aurora`
binary wires everything together.

| Crate                    | Role                                                                          |
|--------------------------|-------------------------------------------------------------------------------|
| `aurora`                 | CLI binary (`clap`), composition root, WASM plugin loading (`plugins.rs`)     |
| `aurora-core`            | Beamfile parser, DAG engine, scheduler, cache, environment evaluation         |
| `aurora-tui`             | ratatui interface: beam picker and execution/log views                        |
| `aurora-executor-api`    | `Executor` trait + shared I/O types (the contract between host and executors) |
| `aurora-executor-local`  | Native shell executor (default)                                               |
| `aurora-executor-docker` | Runs commands inside a container via the Docker CLI                           |

### Execution pipeline (the path a run takes)

1. **Parse** (`aurora-core/src/parser/`): a `pest` grammar (`aurora.pest`) parses the `Beamfile` into the AST in
   `ast.rs` (`BeamFile` → `AuroraConfig`, `Variable`, `Environment`, `Beam`). `--var` overrides are applied to
   `Variable.default` in `main.rs` after parsing.
2. **Environment** (`env.rs`): the `environment {}` block is evaluated sequentially; `shell(...)` values are executed
   and each result is visible to later variables. Crucially, the process environment is **not** inherited wholesale:
   only an allowlist (`ENV_ALLOWLIST` plus `LC_*`) is propagated, because a Beamfile is treated as untrusted and beams
   run arbitrary commands (locally and via `docker -e`). Anything else must be declared explicitly.
3. **DAG** (`dag.rs`): `BeamGraph` (petgraph `DiGraph`) where an edge `dep -> beam` means "dep runs first". Cycle
   detection and unknown-dependency errors happen here. `execution_levels(root)` returns the transitive closure of a
   target; `transitive_dependents` is used to cancel a whole downstream subtree on failure. Traversals are iterative (
   explicit stacks) to stay safe on very deep dependency chains.
4. **Schedule** (`scheduler.rs`): event-driven, not level-by-level. The scheduler tracks a remaining in-degree per beam,
   spawns a beam into a tokio `JoinSet` when its in-degree hits zero, and decrements dependents as each finishes.
   `max_parallelism` is enforced with a `Semaphore`. Per-beam outcomes are `Ok`/`Failed`/`Cancelled`; a failed beam
   cancels its transitive dependents. `allow_failure` beams count as `Ok` for scheduling. The scheduler owns the cache
   and consults it before running (skip on valid hash + present outputs).
5. **Execute**: a beam's `run.executor` selects an `Executor` from a name→`Arc<dyn Executor>` map (falling back to
   `local`). Output streams back live through an `mpsc` channel (`ExecutionInput.output_tx`).

### Scheduler ↔ TUI communication

The scheduler and the TUI are decoupled by channels. The scheduler emits `SchedulerEvent`s (`BeamStarted`,
`BeamCompleted`, `BeamOutput`, `AllDone`) over an `mpsc::Sender`; the TUI drains them in its render loop and updates
`ExecutionState`. Cancellation flows the other way: the TUI sends a beam name over an `mpsc::UnboundedSender<String>`,
and the scheduler races each running beam's future against a per-beam `oneshot` (`tokio::select!`). When cancellation
wins, the executor future is dropped, killing the child process (executors spawn with `kill_on_drop`), and `Cancelled`
is emitted.

**Rerun** (`r` in the TUI) is a closure passed from `main.rs` into the TUI: it builds a fresh `Scheduler` for the
focused beam, reusing already-succeeded beams as `pre_success` so they are not re-run, and swaps in the new event/cancel
channels.

### Caching (`cache.rs`)

`BeamCache` stores one JSON entry per beam under `.aurora/cache/`, keyed by a SHA-256 hash of the beam's `inputs` (file
contents + paths, sorted). A cache hit also requires every declared `output` to still exist on disk. On a hit, the beam
is skipped and its cached stdout/stderr are replayed as `BeamOutput` events. Beam names are sanitized into safe file
stems (with a hash suffix) to prevent path traversal from an untrusted Beamfile.

### Executors and WASM plugins

Every executor implements the async `Executor` trait from `aurora-executor-api`. `local` and `docker` are registered in
`main.rs`. WASM/`extism` plugins are supported via `aurora/src/plugins.rs` (`WasmExecutor`, `discover_plugins` reads
`~/.aurora/plugins/*.wasm`); note this loader exists but is not yet wired into the executor map in `main.rs`.

## Conventions

- **Language**: code comments and git commit messages in this repository are written in **English**; user-facing
  surfaces (`README.md`, CLI `--help` text, `docs/`) are also in English.
- **Commits**: gitmoji + Conventional Commits (e.g. `:sparkles: feat(tui): ...`). Never add Claude/Anthropic attribution
  to commits, tags, or PRs.
- This is an MIT-licensed public open-source project under the `jdevelop-io` org.
