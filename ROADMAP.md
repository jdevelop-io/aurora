# Roadmap

Aurora aims to be a serious, modern alternative to `make`, `just` and
`taskfile`: a task runner whose differentiators are a cache that answers "would
this produce the same result?" rather than "did the files change?", real-time
observability (the TUI), and parallel execution by default. Speed is not one of
them, and the benchmarks in `benchmarks/` say so out loud.

This is a living document, not a dated commitment. It records the direction and
the reasoning behind it. Items are grouped by how much they move Aurora toward
parity and then beyond it.

## Shipped

Already in place and differentiating against `make`/`just`/`taskfile`:

- **Definition-aware caching**: a beam is skipped when its declared `inputs` are
  unchanged, its definition (commands, variables, executor and its settings,
  `dir`, declared environment) is unchanged, and its `outputs` are present.
  Measured against the competition (`benchmarks/`), this is the real
  differentiator: change a command without touching its inputs and `make` (which
  compares timestamps) and `task` (which checksums `sources`) both serve a stale
  result. Aurora re-runs. `just` has no cache at all.
- **Real-time TUI** (ratatui): beam picker with fuzzy search, live log
  streaming, per-beam rerun. No mainstream task runner offers this.
- **Parallel DAG scheduling** by default (event-driven, `max_parallelism`
  bounded). Worth stating precisely, because the benchmark deflated the original
  claim: `just` genuinely cannot run dependencies in parallel, and `make` only
  does with an explicit `-j`, but **`task` parallelises its `deps` by default
  too**. The argument is the default and the bound, not throughput.
- **Process-start cost on par with `make`** (`benchmarks/`): a beam needing no
  shell is exec'd directly rather than through `sh -c`, and every program is
  resolved to an absolute path, which lets Rust use `posix_spawn` instead of
  forking a 24 MB address space. At equal features (Aurora always captures each
  beam's output; `make` only does with `--output-sync`) Aurora is the faster of
  the two.
- **`${var.name}` interpolation in commands**, with a hard error on unknown
  variables (both commands and executor configs).
- **Executors**: `local` (native shell), `docker`, and **WASM plugins**
  (extism) discovered from `~/.aurora/plugins/` and registered with
  native-executor precedence.
- **Headless mode for CI**: auto-detected, prefixed per-beam output, ASCII
  recap, correct exit codes. `condition {}` and `skip_if` gating evaluated at
  runtime.
- **Untrusted-Beamfile security posture**: the process environment is not
  inherited wholesale; only an allowlist is propagated.

## Table stakes (ergonomics every user hits on day one)

These close the gap with `just`/`taskfile` for everyday authoring.

- [x] **Per-invocation beam arguments**: `aurora deploy web-01` binds `web-01`
  to the invoked target as `${arg.1}`; `${args}` forwards the whole tail
  (`aurora test -- --nocapture`). Beams may also declare private, local
  `variable` blocks; a global `variable` remains the channel for values shared
  down the dependency chain.
- [x] **Per-beam working directory (`dir`)** — a beam declares `dir = "..."`;
  its run commands, `inputs`/`outputs` and gates all resolve against that
  directory. Relative paths join onto the Beamfile directory, absolute paths
  replace it. Essential in monorepos.

## Modern expectations

Expected of a tool that positions itself as modern.

- [x] **`--watch` mode** — re-run a beam (and dependents) on input changes.
  Pairs naturally with the TUI.
- [ ] **Composition / `include`** — import other Beamfiles or shared task
  libraries. What makes `task` viable in large monorepos.
- [ ] **Distribution and discoverability** — shell completions (`--completions`),
  a man page (`--man`) and a Homebrew formula
  ([`jdevelop-io/homebrew-tap`](https://github.com/jdevelop-io/homebrew-tap),
  refreshed by the release workflow) are in place. Still missing: publishing to
  crates.io, and a user-facing documentation site (reference docs currently live
  inside the Claude Code plugin skill, invisible to a normal user).
- [x] **Machine-readable output (`--json`)**: a streamed NDJSON event feed on
  stdout for CI (lifecycle events, per-beam output and status, and pre-run
  errors), versioned by a `schema` field.

## Differentiators (where Aurora can lead)

Directions `make`/`just`/`taskfile` do not target.

- [ ] **Remote / shared cache** — promote the existing local input-hash cache
  toward a shared or distributed cache (Turbo/Nx tier). The strongest
  long-term bet.
- [ ] **Loops / matrix** — `for` over a list for matrix-style builds in CI.

## Non-goals (for now)

- Feature-for-feature parity with `make`'s pattern rules and text functions.
  Aurora competes on parallelism, observability and caching, not on being a
  drop-in `make` replacement.
