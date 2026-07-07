---
name: using-aurora
description: This skill should be used when a Beamfile is present in the project, or when the user mentions Aurora, beams, the task runner, or asks to "build", "run", "test", "list", "create a beam", or "edit the Beamfile" in a project that uses Aurora. Covers the Beamfile DSL, the aurora CLI, and Aurora's execution model.
---

# Using Aurora

Aurora is a task runner and build tool written in Rust, an alternative to `make`, `just`, and `taskfile`. Tasks are
called **beams** and are declared in a `Beamfile` (an HCL-inspired DSL).

## When to use

- A `Beamfile` exists at the project root.
- The user asks to build, run, test, lint, or list tasks in an Aurora project.
- The user asks to create or edit beams, or to migrate another task runner to Aurora.

## Mental model

- **Beams** are named tasks declared in a `Beamfile`. Each beam runs one or more shell commands.
- **Dependencies form a DAG.** `depends_on` declares prerequisites; Aurora runs independent beams in parallel (bounded
  by the optional `max_parallelism`) and a dependency before its dependents. Cycles are an error. When a beam fails and
  is not `allow_failure`, its entire transitive-dependent subtree is cancelled and never runs; an `allow_failure`
  beam's failure counts as success for scheduling, so its dependents still run.
- **Caching.** A beam is skipped when the SHA-256 hash of its declared `inputs` (glob patterns; file contents and
  paths) is unchanged AND every declared `output` still exists on disk. A beam with no `inputs`, or whose globs match
  no file, is never cached and always runs. Positional arguments are part of the invoked target's cache key, and a
  beam's `dir` moves where its `inputs`/`outputs` resolve. Wrong `inputs`/`outputs` mislead the cache: too few inputs
  means stale results, missing outputs means needless reruns.
- **Executors** decide where commands run: `local` (the default native shell) and `docker` (inside a container via the
  Docker CLI). WASM/extism plugins discovered under `~/.aurora/plugins/*.wasm` are also registered as executors, after
  the native ones and without shadowing a built-in, so `local` and `docker` always keep their meaning. Non-local
  executors are sandboxed: `docker` runs with `--security-opt no-new-privileges` and rejects dangerous volume mounts
  (the Docker socket, `/proc`, `/sys`, `/etc`, the host root); WASM plugins run untrusted with WASI disabled (no
  filesystem or network access), a 512 MiB memory cap, and a 300-second timeout.
- **The process environment is NOT inherited wholesale.** Only an allowlist is propagated to beams (a Beamfile is
  treated as untrusted). Anything a beam needs must be declared in the `environment {}` block or passed with `--var`.

## Workflow

1. **Discover** what exists: read the `Beamfile`, or run `aurora --list` to see beams and descriptions.
2. **Run** a beam: `aurora <beam>`. With no beam argument on a TTY, the picker TUI opens (fuzzy search); in headless
   mode (no TTY, or `--no-tui`) the `default` from the `aurora {}` block is used to run when no beam is given.
   Aurora auto-detects the output mode: a TTY shows the ratatui TUI; a pipe or CI runs headless with plain
   prefixed logs and a meaningful exit code (`--no-tui` forces headless, `-i` forces the TUI).
3. **Preview** without executing: `aurora --dry-run` resolves the target and DAG and prints what would run.
4. **Override** variables at the command line: `aurora --var key=value` (repeatable).
5. **Bypass the cache** when needed: `aurora --no-cache`.

## Writing or editing a Beamfile

Read `references/beamfile-dsl.md` for the full grammar before writing or substantially editing a `Beamfile`. Set
`inputs` and `outputs` deliberately so caching is correct. Declare every environment variable a command relies on.

## CLI details

Read `references/cli.md` for the complete flag set and behaviours.

## Common pitfalls

- Misjudging a `condition {}` block: it is evaluated at runtime before the beam runs (`any` needs one clause to exit
  zero, `all` needs every clause), and the beam is skipped when it is not met. `skip_if` is the single-command
  shorthand, evaluated first.
- Relying on an environment variable without declaring it in `environment {}` or passing `--var`.
- Referencing a beam name in `depends_on` that does not exist (DAG error) or that forms a cycle.
- Assuming a `skip_if`/`condition` gate runs inside the beam's executor: gates always run on the local host (they
  decide whether to run the beam using host state), even when the beam's `run` uses `docker` or a WASM plugin.

## Additional resources

- **`references/beamfile-dsl.md`** — the complete Beamfile grammar (blocks, beam fields, conditions, executors), with
  worked examples. Read it before writing or substantially editing a `Beamfile`.
- **`references/cli.md`** — every CLI flag and behaviour, with examples. Read it for the full command surface.
