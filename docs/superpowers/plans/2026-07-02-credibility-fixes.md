# Credibility Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Aurora's behaviour match its promises: interpolate Beamfile variables inside commands, wire in WASM plugins, and reconcile the documentation with the code.

**Architecture:** Three independent workstreams. (A) extends the existing post-parse `resolve_variables` pass to interpolate `${var.name}` inside `run.commands` and to fail fast on unknown references. (B) promotes the `plugins` module to the library, splits filesystem discovery from registration logic, and wires discovered `.wasm` executors into the executor map with native-executor precedence. (C) corrects the stale reference docs and adds a `--no-cache` regression test.

**Tech Stack:** Rust (Cargo workspace), `pest` parser, `tokio`, `extism` (WASM), `clap`, `anyhow`. Integration tests live in each crate's `tests/` directory (never inline `#[cfg(test)]`).

## Global Constraints

- All source, comments, commit messages, and docs in **English**.
- `cargo clippy --workspace -- -D warnings` must pass (warnings are errors).
- `cargo fmt --all` must leave the tree clean.
- Tests are integration tests under `crates/<crate>/tests/`, exercising the public API. No inline `#[cfg(test)]` modules.
- Commits: gitmoji + Conventional Commits (e.g. `:sparkles: feat(parser): ...`). Never add Claude/Anthropic attribution.
- Do not use the repository git identity override flags.

---

### Task 1: `${var.name}` interpolation in commands with fail-fast on unknown variables

Extends `resolve_variables` to interpolate `${var.<ident>}` inside every `run.commands` entry, changes its return type to `Result<()>`, and makes an unknown variable reference a hard error in both commands and executor configs.

**Files:**
- Modify: `crates/aurora-core/src/parser/mod.rs` (the `resolve_variables` function, ~line 60)
- Modify: `crates/aurora/src/main.rs:87` (add `?` to the call)
- Test: `crates/aurora-core/tests/parser_test.rs` (update 2 existing callers, add new tests)

**Interfaces:**
- Produces: `pub fn resolve_variables(beam_file: &mut BeamFile) -> anyhow::Result<()>` (was `-> ()`).
- Consumes: `BeamFile`, `Beam`, `Run`, `Variable` from `aurora_core::ast` (unchanged).

- [ ] **Step 1: Write the failing tests**

Add to `crates/aurora-core/tests/parser_test.rs`:

```rust
#[test]
fn interpolates_var_in_command() {
    let input = r#"
variable "profile" { default = "release" }
beam "build" {
  run { commands = ["cargo build --profile ${var.profile} for ${var.profile}"] }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["cargo build --profile release for release"]
    );
}

#[test]
fn interpolation_leaves_shell_expansion_untouched() {
    let input = r#"
variable "profile" { default = "release" }
beam "build" {
  run { commands = ["echo ${HOME} ${var.profile} ${OTHER}"] }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["echo ${HOME} release ${OTHER}"]
    );
}

#[test]
fn interpolation_honors_overridden_default() {
    let input = r#"
variable "profile" { default = "debug" }
beam "build" { run { commands = ["build ${var.profile}"] } }
"#;
    let mut bf = parse(input).unwrap();
    // Simulate a --var override applied post-parse.
    bf.variables.iter_mut().find(|v| v.name == "profile").unwrap().default = "release".to_string();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["build release"]
    );
}

#[test]
fn unknown_var_in_command_is_error() {
    let input = r#"
beam "build" { run { commands = ["build ${var.missing}"] } }
"#;
    let mut bf = parse(input).unwrap();
    let err = resolve_variables(&mut bf).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("missing"), "message names the variable: {msg}");
    assert!(msg.contains("build"), "message names the beam: {msg}");
}

#[test]
fn unknown_var_in_executor_config_is_error() {
    let input = r#"
beam "build" {
  run {
    commands = ["cargo build"]
    executor "docker" { image = var.missing }
  }
}
"#;
    let mut bf = parse(input).unwrap();
    let err = resolve_variables(&mut bf).unwrap_err();
    assert!(err.to_string().contains("missing"), "{}", err);
}
```

Also update the two existing callers in the same file so the suite compiles:
- `crates/aurora-core/tests/parser_test.rs:118` — change `resolve_variables(&mut bf);` to `resolve_variables(&mut bf).unwrap();`
- `crates/aurora-core/tests/parser_test.rs:139` — change `resolve_variables(&mut bf);` to `resolve_variables(&mut bf).unwrap();`

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p aurora-core --test parser_test`
Expected: FAIL — compile error (`resolve_variables` returns `()`, so `.unwrap()` does not compile). This is the red state.

- [ ] **Step 3: Implement interpolation and fail-fast**

Replace the whole `resolve_variables` function (and its doc comment) in `crates/aurora-core/src/parser/mod.rs` with:

```rust
/// Resolves variable references now that any `--var` override has been applied
/// to `Variable.default`.
///
/// Two forms are handled:
/// - inside `run.commands`, the embedded token `${var.<name>}` is replaced by
///   the variable's value; any other `${...}` (for example `${HOME}`) is left
///   untouched for the shell;
/// - inside an executor config, a field whose whole value is `var.<name>` is
///   replaced.
///
/// An unknown variable reference is a hard error: a silent typo inside a shell
/// command is a trap.
pub fn resolve_variables(beam_file: &mut BeamFile) -> Result<()> {
    let vars: HashMap<String, String> = beam_file
        .variables
        .iter()
        .map(|v| (v.name.clone(), v.default.clone()))
        .collect();

    for beam in &mut beam_file.beams {
        let beam_name = beam.name.clone();
        if let Some(run) = &mut beam.run {
            for cmd in &mut run.commands {
                *cmd = interpolate_command(cmd, &vars, &beam_name)?;
            }
            if let Some(exec_cfg) = &mut run.executor {
                for val in exec_cfg.config.values_mut() {
                    if let Some(var_name) = val.strip_prefix("var.") {
                        match vars.get(var_name) {
                            Some(resolved) => *val = resolved.clone(),
                            None => bail!(
                                "unknown variable '{}' referenced in beam '{}'",
                                var_name,
                                beam_name
                            ),
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Interpolates `${var.<name>}` tokens in `s`. Non-`var` `${...}` sequences are
/// copied verbatim so shell parameter expansion still works. An unknown
/// variable is a hard error identified by `beam`.
fn interpolate_command(
    s: &str,
    vars: &HashMap<String, String>,
    beam: &str,
) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        // `$` and `{` are ASCII, so byte checks keep `i` on a char boundary.
        if bytes[i] == b'$' && i + 1 < s.len() && bytes[i + 1] == b'{' {
            if let Some(rel) = s[i + 2..].find('}') {
                let end = i + 2 + rel;
                let inner = &s[i + 2..end];
                if let Some(name) = inner.strip_prefix("var.") {
                    if is_ident(name) {
                        match vars.get(name) {
                            Some(v) => {
                                out.push_str(v);
                                i = end + 1;
                                continue;
                            }
                            None => bail!(
                                "unknown variable '{}' referenced in beam '{}'",
                                name,
                                beam
                            ),
                        }
                    }
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// True when `s` matches the grammar's `ident` rule
/// (`ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "_" | "-")*`).
fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
```

Then update the binary caller at `crates/aurora/src/main.rs:87`:

```rust
    // Resolve `var.<name>` references now that any --var override has been
    // applied, so the overrides actually take effect.
    aurora_core::parser::resolve_variables(&mut beam_file)?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p aurora-core --test parser_test`
Expected: PASS (all tests, including the two updated existing ones).

- [ ] **Step 5: Verify the binary still compiles and lints**

Run: `cargo clippy --workspace -- -D warnings && cargo fmt --all --check`
Expected: no warnings, no formatting diff.

- [ ] **Step 6: Commit**

```bash
git add crates/aurora-core/src/parser/mod.rs crates/aurora-core/tests/parser_test.rs crates/aurora/src/main.rs
git commit -m ":sparkles: feat(parser): interpolate \${var.name} in beam commands"
```

---

### Task 2: Wire discovered WASM plugins into the executor map

Promotes the `plugins` module to the library so it is testable, splits directory discovery from registration, registers discovered `.wasm` executors with native-executor precedence, and removes the "not yet wired" dead-code apology.

**Files:**
- Modify: `crates/aurora/src/plugins.rs` (add `discover_plugins_in`, `register_plugins`; imports)
- Modify: `crates/aurora/src/lib.rs` (add `pub mod plugins;`)
- Modify: `crates/aurora/src/main.rs` (remove `#[allow(dead_code)] mod plugins;` and the top comment; wire registration)
- Test: `crates/aurora/tests/plugins_test.rs` (new)

**Interfaces:**
- Produces:
  - `pub fn discover_plugins_in(dir: &std::path::Path) -> Vec<(String, std::path::PathBuf)>`
  - `pub fn discover_plugins() -> Vec<(String, std::path::PathBuf)>` (unchanged signature; now delegates)
  - `pub fn register_plugins(executors: &mut std::collections::HashMap<String, std::sync::Arc<dyn aurora_executor_api::Executor>>, discovered: Vec<(String, std::path::PathBuf)>) -> Vec<String>` — returns the names actually registered (collisions and load failures are skipped with an stderr warning).
- Consumes: `WasmExecutor::load(name: String, path: PathBuf) -> Result<Self>` (already exists; only checks the path exists).

- [ ] **Step 1: Write the failing tests**

Create `crates/aurora/tests/plugins_test.rs`:

```rust
use aurora::plugins::{discover_plugins_in, register_plugins};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

#[test]
fn discover_ignores_non_wasm_and_missing_dir() {
    let missing = std::path::Path::new("/definitely/not/here/aurora-plugins");
    assert!(discover_plugins_in(missing).is_empty());

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("alpha.wasm"), b"\0asm").unwrap();
    fs::write(dir.path().join("notes.txt"), b"nope").unwrap();

    let found = discover_plugins_in(dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "alpha");
}

#[test]
fn register_adds_new_plugins_and_skips_builtin_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let local_path = dir.path().join("local.wasm");
    let extra_path = dir.path().join("extra.wasm");
    fs::write(&local_path, b"\0asm").unwrap();
    fs::write(&extra_path, b"\0asm").unwrap();

    let mut executors: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    executors.insert("local".to_string(), Arc::new(LocalExecutor::new()));

    let registered = register_plugins(
        &mut executors,
        vec![
            ("local".to_string(), local_path),
            ("extra".to_string(), extra_path),
        ],
    );

    assert_eq!(registered, vec!["extra".to_string()]);
    assert!(executors.contains_key("extra"));
    assert_eq!(executors.len(), 2);
}
```

Add `tempfile` as a dev-dependency of the `aurora` crate if it is not already present. Check first:

Run: `grep -n "tempfile" crates/aurora/Cargo.toml`
If absent, add under `[dev-dependencies]`: `tempfile = "3"` (match the version already used elsewhere in the workspace, e.g. `crates/aurora/tests/headless_cli_test.rs` uses it, so it is likely already declared).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p aurora --test plugins_test`
Expected: FAIL — compile error (`aurora::plugins` is not public; `discover_plugins_in` and `register_plugins` do not exist).

- [ ] **Step 3: Refactor and extend `plugins.rs`**

At the top of `crates/aurora/src/plugins.rs`, extend the imports:

```rust
use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
```

Replace the existing `discover_plugins` function with a split pair:

```rust
pub fn discover_plugins() -> Vec<(String, PathBuf)> {
    let plugins_dir = dirs::home_dir()
        .map(|h| h.join(".aurora/plugins"))
        .unwrap_or_default();
    discover_plugins_in(&plugins_dir)
}

/// Lists `*.wasm` files in `dir` as `(name, path)` pairs, where `name` is the
/// file stem. A missing directory yields an empty list. Split from
/// [`discover_plugins`] so the discovery logic is testable without a real home
/// directory.
pub fn discover_plugins_in(dir: &Path) -> Vec<(String, PathBuf)> {
    if !dir.exists() {
        return vec![];
    }

    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "wasm" {
                let name = path.file_stem()?.to_string_lossy().to_string();
                Some((name, path))
            } else {
                None
            }
        })
        .collect()
}

/// Registers each discovered plugin into `executors`. A native (built-in)
/// executor always wins: a plugin whose name is already taken is skipped with
/// an stderr warning, and so is a plugin that fails to load. Returns the names
/// actually registered.
pub fn register_plugins(
    executors: &mut HashMap<String, Arc<dyn Executor>>,
    discovered: Vec<(String, PathBuf)>,
) -> Vec<String> {
    let mut registered = Vec::new();
    for (name, path) in discovered {
        if executors.contains_key(&name) {
            eprintln!(
                "aurora: ignoring plugin '{}' ({}): a built-in executor already uses that name",
                name,
                path.display()
            );
            continue;
        }
        match WasmExecutor::load(name.clone(), path.clone()) {
            Ok(executor) => {
                executors.insert(name.clone(), Arc::new(executor) as Arc<dyn Executor>);
                registered.push(name);
            }
            Err(e) => eprintln!(
                "aurora: skipping plugin '{}' ({}): {}",
                name,
                path.display(),
                e
            ),
        }
    }
    registered
}
```

- [ ] **Step 4: Promote the module to the library**

In `crates/aurora/src/lib.rs`, add (next to the existing module declarations):

```rust
pub mod plugins;
```

In `crates/aurora/src/main.rs`, delete the top block (lines 1-5):

```rust
// WASM plugin loader present but not yet wired into the executor map
// (see CLAUDE.md): dead code is tolerated as long as it is not wired up,
// rather than removing it or wiring it prematurely.
#[allow(dead_code)]
mod plugins;
```

The `main.rs` file keeps using the module through the library path `aurora::plugins`.

- [ ] **Step 5: Wire registration into `main.rs`**

Immediately after the executor map is built (the loop ending at `crates/aurora/src/main.rs:159`), insert:

```rust
    // Register community WASM executors discovered under ~/.aurora/plugins.
    // Native executors take precedence: a plugin cannot shadow local/docker.
    aurora::plugins::register_plugins(&mut executors, aurora::plugins::discover_plugins());
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p aurora --test plugins_test`
Expected: PASS (both tests).

- [ ] **Step 7: Verify the whole workspace compiles and lints**

Run: `cargo clippy --workspace -- -D warnings && cargo fmt --all --check`
Expected: no warnings (in particular the `plugins` module is no longer dead code), no formatting diff.

- [ ] **Step 8: Commit**

```bash
git add crates/aurora/src/plugins.rs crates/aurora/src/lib.rs crates/aurora/src/main.rs crates/aurora/tests/plugins_test.rs crates/aurora/Cargo.toml
git commit -m ":sparkles: feat(cli): register discovered WASM plugins as executors"
```

---

### Task 3: Reconcile documentation with the code and guard `--no-cache`

Adds a `--no-cache` regression test (the `--dry-run` by-level behaviour is already covered by `crates/aurora/tests/headless_cli_test.rs::dry_run_prints_execution_plan_by_level`), then corrects every stale documentation claim.

**Files:**
- Test: `crates/aurora/tests/headless_cli_test.rs` (add one test)
- Modify: `claude-code-plugin/skills/using-aurora/references/cli.md`
- Modify: `claude-code-plugin/skills/using-aurora/references/beamfile-dsl.md`
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Modify: `claude-code-plugin/skills/using-aurora/SKILL.md` (only where it repeats a corrected claim)

- [ ] **Step 1: Write the failing `--no-cache` test**

Append to `crates/aurora/tests/headless_cli_test.rs`:

```rust
#[test]
fn no_cache_writes_no_cache_directory() {
    let dir = fixture_dir(BEAMFILE);

    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--no-tui", "--no-cache"])
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
    let dir = fixture_dir(BEAMFILE);

    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["ok", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "run should succeed");

    assert!(
        dir.path().join(".aurora/cache").exists(),
        "a normal run persists a cache directory (proving --no-cache matters)"
    );
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p aurora --test headless_cli_test no_cache default_run`
Expected: PASS. These assert the *current* (correct) behaviour — they are the regression guard that keeps the docs true. If either fails, stop: the code does not match what the docs will claim, and the discrepancy must be resolved before editing docs.

- [ ] **Step 3: Fix `cli.md`**

In `claude-code-plugin/skills/using-aurora/references/cli.md`:

Replace the `--dry-run` bullet:

> - `--dry-run` — resolve the target beam name (honouring `default` when no beam is given) and print `Would execute beam: <name>`, then exit without running anything. It prints only the target name, not the full DAG.

with:

> - `--dry-run` — build the DAG for the target beam (honouring `default` when no beam is given) and print the execution plan grouped by dependency level (`Execution plan for '<target>':` then one `level N: a, b` line per level), then exit without running anything. Building the DAG here also surfaces a malformed Beamfile (cycle, unknown dependency).

Replace the `--no-cache` bullet:

> - `--no-cache` — currently has no effect: the flag is accepted but not yet wired, so caching is always applied. To force a beam to re-run, change one of its declared `inputs` or delete its entry under `.aurora/cache/`.

with:

> - `--no-cache` — ignore the cache for this run: no entry is read and none is persisted, so no `.aurora/cache` directory is written. Every beam runs regardless of unchanged inputs.

- [ ] **Step 4: Fix `beamfile-dsl.md`**

In `claude-code-plugin/skills/using-aurora/references/beamfile-dsl.md`, replace the `condition` note:

> Note: the `condition {}` block is currently parsed but not yet evaluated at runtime, so it has no effect today. For conditional execution, use `skip_if`. The syntax is documented here for forward compatibility.

with:

> The `condition {}` block is evaluated at runtime, before the beam runs: `any` succeeds if at least one clause exits zero, `all` requires every clause to exit zero. When the condition is not met the beam is skipped. `skip_if` is the single-command shorthand and is evaluated first.

Also, in the same file, extend the `run` block section to document command interpolation. After the sentence "When no `executor` is given, the `local` executor (native shell) is used." add:

> Inside `commands`, `${var.<name>}` is replaced by the value of the Beamfile variable `<name>` (honouring `--var` overrides). Any other `${...}` is passed through to the shell unchanged, so `${HOME}` and environment variables from the `environment {}` block still expand normally. Referencing an undeclared variable is an error.

- [ ] **Step 5: Fix `README.md`**

In `README.md`, replace the `--dry-run` example block (currently showing `fmt → run` / `clippy → run (depends_on: fmt)` output) with the real per-level output:

```
$ aurora check --dry-run
Execution plan for 'check':
  level 0: fmt
  level 1: clippy, test
  level 2: check
```

In the "A beam can declare" list, the `run` bullet already mentions commands; add a sentence to the paragraph that starts "Variables are declared with `variable {}`":

> Inside a beam's `commands`, `${var.name}` is interpolated with the variable's value (after any `--var` override); other `${...}` sequences are left for the shell.

The Features list already advertises "WASM plugins via extism"; leave it, it is now accurate (discovered `~/.aurora/plugins/*.wasm` are registered as executors, native executors taking precedence). No change needed there.

- [ ] **Step 6: Fix `CLAUDE.md`**

In `CLAUDE.md`, in the "Executors and WASM plugins" section, replace:

> WASM/`extism` plugins are supported via `aurora/src/plugins.rs` (`WasmExecutor`, `discover_plugins` reads `~/.aurora/plugins/*.wasm`); note this loader exists but is not yet wired into the executor map in `main.rs`.

with:

> WASM/`extism` plugins are supported via `aurora/src/plugins.rs` (`WasmExecutor`, `discover_plugins` reads `~/.aurora/plugins/*.wasm`). `main.rs` registers them into the executor map after the native executors via `register_plugins`, which skips any name that collides with a built-in so `local`/`docker` cannot be shadowed.

- [ ] **Step 7: Scan `SKILL.md` for repeated stale claims**

Run: `grep -n "no effect\|not yet\|not wired\|only the target\|not evaluated" claude-code-plugin/skills/using-aurora/SKILL.md`
For each hit that repeats a claim corrected above (`--no-cache`, `--dry-run`, `condition`, WASM wiring), edit it to match the corrected wording. If there are no hits, make no change to this file.

- [ ] **Step 8: Verify the reference tests still pass and the tree is clean**

Run: `cargo test -p aurora --test headless_cli_test && cargo fmt --all --check`
Expected: PASS, no formatting diff.

- [ ] **Step 9: Commit (test, then docs)**

```bash
git add crates/aurora/tests/headless_cli_test.rs
git commit -m ":white_check_mark: test(cli): assert --no-cache persists no cache directory"

git add README.md CLAUDE.md claude-code-plugin/skills/using-aurora/references/cli.md claude-code-plugin/skills/using-aurora/references/beamfile-dsl.md claude-code-plugin/skills/using-aurora/SKILL.md
git commit -m ":memo: docs: reconcile references with actual CLI behaviour"
```

---

## Final verification

- [ ] **Run the full workspace suite and lints**

Run: `cargo fmt --all --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`
Expected: everything green.
