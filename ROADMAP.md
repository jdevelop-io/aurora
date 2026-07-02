# Roadmap

Aurora aims to be a serious, modern alternative to `make`, `just` and
`taskfile`: a task runner whose differentiators are first-class parallel
execution, real-time observability (the TUI), and caching that can grow from
local to distributed.

This is a living document, not a dated commitment. It records the direction and
the reasoning behind it. Items are grouped by how much they move Aurora toward
parity and then beyond it.

## Shipped

Already in place and differentiating against `make`/`just`/`taskfile`:

- **Parallel DAG scheduling** by default (event-driven, `max_parallelism`
  bounded). `just` runs dependencies sequentially; `make -j` is fragile.
- **Real-time TUI** (ratatui): beam picker with fuzzy search, live log
  streaming, per-beam rerun. No mainstream task runner offers this.
- **Input-hash caching**: a beam is skipped when its declared `inputs` are
  unchanged and its `outputs` are present.
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

- [ ] **Per-invocation beam arguments** — `aurora deploy staging` passing
  `staging` to the beam, beyond the current global `--var`. Both `just` and
  `task` support this; it is the most-requested missing ergonomic.
- [ ] **Per-beam working directory (`dir`)** — the `local` executor currently
  runs every beam from the Beamfile directory; `docker` already has `workdir`.
  Essential in monorepos.

## Modern expectations

Expected of a tool that positions itself as modern.

- [ ] **`--watch` mode** — re-run a beam (and dependents) on input changes.
  Pairs naturally with the TUI.
- [ ] **Composition / `include`** — import other Beamfiles or shared task
  libraries. What makes `task` viable in large monorepos.
- [ ] **Distribution and discoverability** — shell completions, a Homebrew
  formula, a man page, and a user-facing documentation site (reference docs
  currently live inside the Claude Code plugin skill, invisible to a normal
  user).

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
