# Credibility fixes — design

Date: 2026-07-02
Status: approved (design), pending implementation plan

## Goal

Make Aurora's promises match its behaviour. Three defects erode credibility
for a tool that positions itself as a serious alternative to `make`, `just`
and `taskfile`:

1. Beamfile variables cannot be interpolated inside commands, so `var.*` is
   almost useless outside executor configuration.
2. The WASM plugin loader exists but is never wired into the executor map, so
   the advertised "WASM plugins" feature does nothing.
3. The reference documentation contradicts the code (stale claims about
   `--no-cache`, `--dry-run` and `condition {}`).

Scope is limited to these three workstreams. No new user-facing feature beyond
command interpolation.

## Workstream A — `${var.name}` interpolation in commands

### Where

Extend `resolve_variables` in `crates/aurora-core/src/parser/mod.rs`. It is
already called from `crates/aurora/src/main.rs` after `--var` overrides are
applied, and it already resolves `var.<name>` references inside executor
configuration. It will additionally interpolate `${var.<ident>}` inside
`run.commands`. Reusing this single entry point guarantees consistency and
keeps `--var` overrides effective.

### Semantics

- Only the exact pattern `${var.<ident>}` is substituted.
- Any other `${...}` (for example `${HOME}`, `${GIT_SHA}`) passes through
  untouched to the shell. There is no collision with shell parameter
  expansion.
- `environment {}` values remain accessible via `$VAR`: they are already
  exported as real process environment variables (`ExecutionInput.env`,
  `.envs(&input.env)` in the local executor), so they need no interpolation.

### Unknown variable

Referencing an undeclared variable is a hard error raised at load time, with a
clear message identifying the variable and the beam, for example:

```
unknown variable 'profil' referenced in beam 'deploy'
```

Executor-configuration resolution is aligned to the same behaviour. Today it
silently leaves an unresolved `var.name` string in place; after this change an
unknown reference is an error there too. This fail-fast behaviour is the point
of the workstream: a silent typo inside a shell command is a trap.

Because unknown references now error, `resolve_variables` changes signature to
return a `Result` (it is currently infallible). Its single call site in
`main.rs` propagates the error with `?`.

### Tests

- Single reference substituted correctly.
- Multiple references in one command string.
- `${HOME}` and other non-`var` `${...}` left intact.
- Interaction with `--var` (override reflected in the interpolated command).
- Unknown variable in a command → error.
- Unknown variable in an executor config → error (alignment).

## Workstream B — Wire in WASM plugins

### Where

In `crates/aurora/src/main.rs`, after `local` and `docker` are registered
(around lines 152-159): call `plugins::discover_plugins()`, then
`WasmExecutor::load(name, path)` for each discovered `.wasm`, and insert the
result into the executor map.

### Precedence

Native executors win. A plugin whose name collides with a built-in
(`local`, `docker`) must not hijack it: a plugin is inserted only when its name
is not already taken, and a collision emits a warning on stderr.

### Robustness

A `.wasm` that is missing or fails to load does not abort the run: it emits a
warning on stderr and the run continues with the remaining executors.

### Cleanup

Remove the `#[allow(dead_code)]` attribute and the top-of-file comment
(`main.rs` lines 1-5) that explain why the loader is not wired.

### Tests

- `discover_plugins` on a missing directory returns empty.
- `discover_plugins` on an empty directory returns empty.
- Precedence: a plugin named like a built-in does not replace the built-in.

Real `.wasm` execution through extism is out of scope for these tests; only the
discovery and registration logic is covered.

## Workstream C — Reconcile documentation with code

Factual corrections only, no cosmetic rewrite.

- `claude-code-plugin/skills/using-aurora/references/cli.md`:
  - `--no-cache` is wired (drop "currently has no effect").
  - `--dry-run` prints the full DAG grouped by level
    (`Execution plan for '<t>': level N: ...`), not only the target name.
- `claude-code-plugin/skills/using-aurora/references/beamfile-dsl.md`:
  - `condition {}` is evaluated at runtime (drop "parsed but not yet
    evaluated").
- `README.md`:
  - Replace the `--dry-run` example with the real per-level output.
  - Add `${var.name}` command interpolation to the Beamfile section.
  - Confirm WASM plugins as active.
- `CLAUDE.md` (project root):
  - Drop "this loader exists but is not yet wired into the executor map".
- `claude-code-plugin/skills/using-aurora/SKILL.md`:
  - Align with the corrections above where it repeats these claims.

### Anti-drift regression test

Add an integration test (in the `aurora` crate) that exercises the CLI surface
whose documentation drifted, so documentation and behaviour cannot silently
diverge again:

- `--dry-run` prints an execution plan containing the expected beams grouped by
  level.
- `--no-cache` is accepted and disables cache persistence for the run.

## Order and method

TDD (red, green, refactor) for A then B; C last (documentation, validated
against the code). One commit per workstream, gitmoji + Conventional Commits.

## Out of scope

- Per-beam working directory (`dir`).
- Per-invocation beam arguments.
- Watch mode, includes, remote cache.
- Any change to the `condition {}` grammar or evaluation (it already works).
