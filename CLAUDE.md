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
cargo test -p aurora-runner-core                # one crate
cargo test -p aurora-runner-core scheduler      # tests in an integration test file (tests/scheduler_test.rs)
cargo test -p aurora-runner-tui log_search      # filter by substring
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

The version lives in the root `Cargo.toml` alone; every crate inherits it via `version.workspace = true`. It appears
there six times: once under `[workspace.package]`, and once per internal `aurora-runner-*` path dependency under
`[workspace.dependencies]` (a path dependency needs a version to be publishable). They must be bumped together, which
is what the `release` command does. Pushing a `v*` git tag triggers `.github/workflows/release.yml`, which cross-builds
binaries for Linux/macOS/Windows and publishes a GitHub release with SHA-256 sums. Use the `release` skill (or
`/release <X.Y.Z|patch|minor|major>` command) to bump, validate, commit, tag, and push, rather than doing it by hand.

**Crate names**: the packages are published under the `aurora-runner-*` namespace, because `aurora` and `aurora-core`
were already taken on crates.io. The binary (`aurora`) and every Rust crate name (`aurora_core`, `aurora_tui`, ...) are
unchanged: only the package names differ, pinned by an explicit `[lib] name`. So `use aurora_core::...` in code, but
`cargo test -p aurora-runner-core` on the command line. The MSRV is 1.91, imposed by the wasmtime/extism chain.

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
   `ast.rs` (`BeamFile` → `AuroraConfig`, `Variable`, `Environment`, `Beam`, and a `Beam`'s own `Param` list and
   optional per-beam `Environment` overlay). `--var` overrides are applied to `Variable.default` in `main.rs` after
   parsing, then `parser::resolve_variables` interpolates `${var.x}` into every beam's `dir`, `skip_if`, `condition`,
   `run.commands`, executor config and bound `depends_on` values (a beam's own `environment {}` values are not part
   of this pass; see step 2).
2. **Environment** (`env.rs`): the top-level `environment {}` block is evaluated sequentially; `shell(...)` values are
   executed and each result is visible to later variables. Crucially, the process environment is **not** inherited
   wholesale: only an allowlist (`ENV_ALLOWLIST` plus `LC_*`) is propagated, because a Beamfile is treated as
   untrusted and beams run arbitrary commands (locally and via `docker -e`). Anything else must be declared
   explicitly.
3. **Expand** (`expand.rs`): monomorphizes parameterized beams into fully resolved instances, run once the invoked
   target and its `--var`/CLI arguments are known and before the DAG is built. `bind_cli_args` binds the target's
   CLI arguments to its declared `param`s (positional-then-named: `name=value` binds by name, everything else fills
   the remaining params in declaration order; a required param left unbound, a surplus argument, or arguments to a
   param-less beam are hard errors). `expand` then walks the target's dependency closure: each `depends_on` edge is
   resolved into the child's binding set by `bind_edge` (explicit `params = { ... }` entries, interpolated in the
   parent's own bindings, plus the child's defaults; an unbound required param with no explicit binding is a hard
   error naming the missing param and the object form to add), instantiated (`instantiate`, which stamps the
   instance id and interpolates `${param.x}` into every field that reaches a shell, an executor or the beam's own
   `environment {}` overlay), and deduplicated when two edges resolve to the identical `(beam, bindings)` pair. A
   divergent chain of ever-distinct bindings is capped at `MAX_INSTANTIATION_DEPTH` (64) and turned into a clear
   error instead of unbounded growth. Every remaining beam without required params also gets a default instance, so
   the picker sidebar can still list and launch it. From here on, the scheduler, cache and TUI operate on plain
   `Beam`s keyed by their instance id, a `String` of the form `name` (no params) or `name[k=v,...]` (bindings sorted
   by key, so the id is stable and two distinct binding sets can never collapse into the same one).
4. **DAG** (`dag.rs`): `BeamGraph` (petgraph `DiGraph`) where an edge `dep -> beam` means "dep runs first", built from
   the expanded instances. Cycle detection and unknown-dependency errors happen here. `execution_levels(root)`
   returns the transitive closure of a target; `transitive_dependents` is used to cancel a whole downstream subtree
   on failure. Traversals are iterative (explicit stacks) to stay safe on very deep dependency chains.
5. **Schedule** (`scheduler.rs`): event-driven, not level-by-level. The scheduler tracks a remaining in-degree per
   instance, spawns one into a tokio `JoinSet` when its in-degree hits zero, and decrements dependents as each
   finishes. `max_parallelism` is enforced with a `Semaphore`. Per-instance outcomes are `Ok`/`Failed`/`Cancelled`; a
   failed instance cancels its transitive dependents. `allow_failure` instances count as `Ok` for scheduling. Right
   before a beam runs, its own `environment {}` overlay (already interpolated per instance in step 3, evaluated
   against the global environment by `apply_env_overlays`) is merged on top of the global environment, shadowing it
   for that instance only, in both execution and the declared environment that feeds the cache key. The scheduler
   owns the cache and consults it before running (skip on valid hash + present outputs).
6. **Execute**: an instance's `run.executor` selects an `Executor` from a name→`Arc<dyn Executor>` map (falling back
   to `local`). Output streams back live through an `mpsc` channel (`ExecutionInput.output_tx`).

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

`BeamCache` stores one JSON entry per **instance** under `.aurora/cache/`, keyed by a SHA-256 hash of two halves:

1. the instance's `inputs` (file contents + paths, sorted), and
2. its **definition** (`BeamDefinition`): the resolved `run.commands`, the executor name and its config, the `dir`,
   the declared `environment {}` values (global plus, for this instance, its own overlay), and its resolved param
   `bindings`.

The key must answer "would running this instance produce the same result?", not just "did its input files change?".
Hashing the inputs alone served the previous run's result after a command edit, a `--var` override, an instance bound
to different param values, or a docker image bump. Variables are *not* hashed separately: `resolve_variables`
interpolates `${var.x}` into `run.commands` (and the other interpolatable fields) before expansion, so the resolved
commands already carry them. `bindings` is folded into the key even though `expand::instantiate` has already
interpolated every `${param.x}` reference into the hashed fields: the invariant that an instance's key always covers
its own bindings must hold even for a param no hashed field happens to reference. The ambient allowlisted environment
(`PATH`, `TERM`, `PWD`, ...) is deliberately *excluded* from the key (`env::declared_only`): it is machine context, and
folding it in would make the key vary between terminals and machines, ruling out the shared cache on the roadmap.

An instance with no `inputs`, or whose globs match no file, has no key and always runs: the definition alone must never
key an entry. A cache hit also requires every declared `output` to still exist on disk. On a hit, the instance is
skipped and its cached stdout/stderr are replayed as `BeamOutput` events. Instance ids are sanitized into safe file
stems (with a hash suffix) to prevent path traversal from an untrusted Beamfile, so `build[version=1.2]` and
`build[version=1.3]` land in distinct cache files.

### Executors and WASM plugins

Every executor implements the async `Executor` trait from `aurora-executor-api`. `local` and `docker` are registered in
`main.rs`. WASM/`extism` plugins are supported via `aurora/src/plugins.rs` (`WasmExecutor`, `discover_plugins` reads
`~/.aurora/plugins/*.wasm`). `main.rs` registers them into the executor map after the native executors via
`register_plugins`, which skips any name that collides with a built-in so `local`/`docker` cannot be shadowed.

## Conventions

- **Language**: code comments and git commit messages in this repository are written in **English**; user-facing
  surfaces (`README.md`, CLI `--help` text, `docs/`) are also in English.
- **Commits**: gitmoji + Conventional Commits (e.g. `:sparkles: feat(tui): ...`). Never add Claude/Anthropic attribution
  to commits, tags, or PRs.
- This is an MIT-licensed public open-source project under the `jdevelop-io` org.
