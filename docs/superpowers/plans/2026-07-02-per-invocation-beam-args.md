# Per-invocation Beam Arguments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a beam receive positional arguments from the command line (`aurora deploy web-01`, read as `${arg.1}`; `aurora test -- --nocapture`, read as `${args}`) and let a beam declare private, local `variable` blocks.

**Architecture:** Two string-interpolation inputs sharing one scanner. Variables (global top-level, and now beam-local shadowing them) resolve at parse time; positional arguments resolve after the CLI target is known, for that target only. Argument values are inserted literally and folded into the target's cache key so a changed argument list re-runs it.

**Tech Stack:** Rust (workspace), `pest` grammar, `clap` 4.5 CLI, `tokio` scheduler, `sha2` cache hashing. Tests are integration tests in each crate's `tests/` directory.

## Global Constraints

- Comments and commit messages in **English**. Commits use gitmoji + Conventional Commits (e.g. `:sparkles: feat(parser): ...`). No Claude/Anthropic attribution anywhere.
- `cargo clippy --workspace -- -D warnings` must stay clean (warnings are errors).
- Tests live in each crate's `tests/` directory as integration tests, exercising the public crate API. Never inline `#[cfg(test)]` modules.
- Interpolation philosophy: an unknown `${var.x}` or a missing `${arg.N}` is a **hard error**, never a silent empty string.
- Argument rules (from the spec): `${arg.N}` is **1-indexed**; `${args}` joins all arguments with a single space and is empty when none are passed; argument text is inserted **literally** (never re-interpolated); arguments reach the **invoked target only** and are interpolated in **`run.commands` only** for v1.
- `--var` overrides **global** variables only; it never reaches a beam-local variable.
- Spec: `docs/superpowers/specs/2026-07-02-per-invocation-beam-args-design.md`.

---

## File Structure

- `crates/aurora-core/src/parser/aurora.pest` — grammar; allow a `variable {}` block inside a beam.
- `crates/aurora-core/src/ast.rs` — `Beam` gains `variables: Vec<Variable>` and `args: Vec<String>`.
- `crates/aurora-core/src/parser/mod.rs` — parse beam-local variables; per-beam variable scoping; shared `${...}` scanner; new `resolve_arguments`.
- `crates/aurora-core/src/cache.rs` — `hash_with_args` folds the argument vector into an inputs hash.
- `crates/aurora-core/src/scheduler.rs` — thread `beam.args` into the cache lookup.
- `crates/aurora/src/main.rs` — clap positional argument capture; call `resolve_arguments` once the target is known; update the virtual `__multi__` beam literal.
- Tests: `crates/aurora-core/tests/parser_test.rs`, `crates/aurora-core/tests/cache_test.rs`, `crates/aurora/tests/args_cli_test.rs` (new).
- Docs: `README.md`, `ROADMAP.md`.

---

## Task 1: Beam-local `variable` blocks

**Files:**
- Modify: `crates/aurora-core/src/parser/aurora.pest:40-50` (the `beam_field` rule)
- Modify: `crates/aurora-core/src/ast.rs:43-58` (the `Beam` struct)
- Modify: `crates/aurora-core/src/parser/mod.rs:65-98` (`resolve_variables`), `236-289` (`parse_beam_block`)
- Modify: `crates/aurora/src/main.rs:119-131` (the `__multi__` virtual beam literal)
- Test: `crates/aurora-core/tests/parser_test.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `Beam.variables: Vec<Variable>` (beam-local variables, empty when none declared). `resolve_variables` now resolves each beam's `${var.x}` against its locals overlaid on the globals (local shadows global).

- [ ] **Step 1: Write the failing test**

Add to `crates/aurora-core/tests/parser_test.rs`:

```rust
#[test]
fn test_parse_beam_local_variable() {
    let input = r#"
beam "deploy" {
  variable "strategy" { default = "rolling" }
  run { commands = ["deploy.sh --strategy ${var.strategy}"] }
}
"#;
    let bf = parse(input).unwrap();
    let beam = &bf.beams[0];
    assert_eq!(beam.variables.len(), 1);
    assert_eq!(beam.variables[0].name, "strategy");
    assert_eq!(beam.variables[0].default, "rolling");
}

#[test]
fn test_local_variable_shadows_global() {
    let input = r#"
variable "strategy" { default = "global" }
beam "deploy" {
  variable "strategy" { default = "local" }
  run { commands = ["echo ${var.strategy}"] }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["echo local"]
    );
}

#[test]
fn test_two_beams_same_local_name_are_independent() {
    let input = r#"
beam "build"  { variable "s" { default = "fast" }    run { commands = ["echo ${var.s}"] } }
beam "deploy" { variable "s" { default = "rolling" } run { commands = ["echo ${var.s}"] } }
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    let build = bf.beams.iter().find(|b| b.name == "build").unwrap();
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    assert_eq!(build.run.as_ref().unwrap().commands, vec!["echo fast"]);
    assert_eq!(deploy.run.as_ref().unwrap().commands, vec!["echo rolling"]);
}

#[test]
fn test_global_var_override_does_not_touch_local() {
    // --var targets globals only; a same-named local keeps its own default.
    let input = r#"
variable "s" { default = "global" }
beam "deploy" {
  variable "s" { default = "local" }
  run { commands = ["echo ${var.s}"] }
}
"#;
    let mut bf = parse(input).unwrap();
    bf.variables[0].default = "overridden".to_string(); // simulates --var s=overridden
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["echo local"]
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p aurora-core --test parser_test test_parse_beam_local_variable`
Expected: FAIL — the field `variables` does not exist on `Beam` (compile error).

- [ ] **Step 3: Add the `variables` field to `Beam`**

In `crates/aurora-core/src/ast.rs`, add the field to the `Beam` struct (after `pub outputs: Vec<String>,`):

```rust
    /// Beam-local variables. Same `variable {}` syntax as the top level, but
    /// scoped to this beam: they shadow a global of the same name and are not
    /// reachable by `--var` (which targets globals only).
    pub variables: Vec<Variable>,
```

- [ ] **Step 4: Allow a `variable` block inside a beam in the grammar**

In `crates/aurora-core/src/parser/aurora.pest`, add `variable_block` to the `beam_field` alternation:

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
    variable_block   |
    beam_run
}
```

- [ ] **Step 5: Parse the local variable and initialise the field**

In `crates/aurora-core/src/parser/mod.rs`, in `parse_beam_block`, add `variables: vec![]` to the `Beam { ... }` initialiser (alongside `dir: None,`), then handle the new rule in the field `match`:

```rust
            Rule::variable_block => {
                beam.variables.push(parse_variable_block(field)?);
            }
```

- [ ] **Step 6: Scope `resolve_variables` to each beam's locals**

In `crates/aurora-core/src/parser/mod.rs`, replace the body of `resolve_variables` so each beam resolves against its locals overlaid on the globals:

```rust
pub fn resolve_variables(beam_file: &mut BeamFile) -> Result<()> {
    let globals: HashMap<String, String> = beam_file
        .variables
        .iter()
        .map(|v| (v.name.clone(), v.default.clone()))
        .collect();

    for beam in &mut beam_file.beams {
        let beam_name = beam.name.clone();
        // Effective scope: a beam-local variable shadows a global of the same
        // name. `--var` only ever changed the globals, so locals stay private.
        let mut vars = globals.clone();
        for local in &beam.variables {
            vars.insert(local.name.clone(), local.default.clone());
        }

        if let Some(dir) = &mut beam.dir {
            *dir = interpolate_command(dir, &vars, &beam_name)?;
        }
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
```

- [ ] **Step 7: Fix the `__multi__` virtual beam literal**

In `crates/aurora/src/main.rs`, the `aurora_core::ast::Beam { ... }` literal (the `__multi__` virtual beam) will no longer compile. Add the field:

```rust
                    variables: vec![],
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p aurora-core --test parser_test`
Expected: PASS (the four new tests and all existing parser tests).

Run: `cargo build --workspace`
Expected: builds clean (confirms the `main.rs` literal was fixed).

- [ ] **Step 9: Commit**

```bash
git add crates/aurora-core/src/parser/aurora.pest crates/aurora-core/src/ast.rs crates/aurora-core/src/parser/mod.rs crates/aurora/src/main.rs crates/aurora-core/tests/parser_test.rs
git commit -m ":sparkles: feat(parser): support beam-local variable blocks"
```

---

## Task 2: Positional argument interpolation (parser layer)

**Files:**
- Modify: `crates/aurora-core/src/ast.rs` (the `Beam` struct — add `args`)
- Modify: `crates/aurora-core/src/parser/mod.rs` (extract the shared scanner; add `resolve_arguments`)
- Modify: `crates/aurora/src/main.rs` (the `__multi__` virtual beam literal)
- Test: `crates/aurora-core/tests/parser_test.rs`

**Interfaces:**
- Consumes: `Beam` from Task 1.
- Produces:
  - `Beam.args: Vec<String>` — the invoked target's argument vector (empty for every other beam).
  - `pub fn resolve_arguments(beam_file: &mut BeamFile, target: &str, args: &[String]) -> anyhow::Result<()>` — interpolates `${arg.N}` / `${args}` in the target's `run.commands`, records `beam.args = args`, and rejects `${arg...}` used in any non-target beam.
  - `fn interpolate_tokens(s: &str, resolve: impl Fn(&str) -> Option<Result<String>>) -> Result<String>` — the shared `${...}` scanner.

- [ ] **Step 1: Write the failing test**

Add to `crates/aurora-core/tests/parser_test.rs` (note the added import on line 1):

```rust
use aurora_core::parser::{parse, resolve_arguments, resolve_variables};
```

```rust
#[test]
fn test_positional_arg_resolves_for_target() {
    let input = r#"beam "deploy" { run { commands = ["deploy.sh ${arg.1} ${arg.2}"] } }"#;
    let mut bf = parse(input).unwrap();
    let args = vec!["web-01".to_string(), "canary".to_string()];
    resolve_arguments(&mut bf, "deploy", &args).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["deploy.sh web-01 canary"]
    );
    assert_eq!(bf.beams[0].args, args);
}

#[test]
fn test_args_whole_tail_joins_with_spaces() {
    let input = r#"beam "test" { run { commands = ["cargo test ${args}"] } }"#;
    let mut bf = parse(input).unwrap();
    let args = vec!["--nocapture".to_string(), "-p".to_string(), "core".to_string()];
    resolve_arguments(&mut bf, "test", &args).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["cargo test --nocapture -p core"]
    );
}

#[test]
fn test_args_empty_when_none_passed() {
    let input = r#"beam "test" { run { commands = ["cargo test ${args}"] } }"#;
    let mut bf = parse(input).unwrap();
    resolve_arguments(&mut bf, "test", &[]).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["cargo test "]
    );
}

#[test]
fn test_missing_arg_index_is_error() {
    let input = r#"beam "deploy" { run { commands = ["deploy ${arg.2}"] } }"#;
    let mut bf = parse(input).unwrap();
    let err = resolve_arguments(&mut bf, "deploy", &["only-one".to_string()]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("arg.2"), "names the index: {msg}");
    assert!(msg.contains("deploy"), "names the beam: {msg}");
}

#[test]
fn test_arg_zero_is_error() {
    let input = r#"beam "deploy" { run { commands = ["deploy ${arg.0}"] } }"#;
    let mut bf = parse(input).unwrap();
    assert!(resolve_arguments(&mut bf, "deploy", &["x".to_string()]).is_err());
}

#[test]
fn test_arg_value_is_inserted_literally_not_reinterpolated() {
    // An argument that itself looks like a token must not be expanded again.
    let input = r#"beam "deploy" { run { commands = ["echo ${arg.1}"] } }"#;
    let mut bf = parse(input).unwrap();
    resolve_arguments(&mut bf, "deploy", &["${var.env}".to_string()]).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["echo ${var.env}"]
    );
}

#[test]
fn test_args_in_non_target_beam_are_rejected() {
    let input = r#"
beam "deploy" { depends_on = ["build"] run { commands = ["deploy ${arg.1}"] } }
beam "build"  { run { commands = ["build ${arg.1}"] } }
"#;
    let mut bf = parse(input).unwrap();
    let err = resolve_arguments(&mut bf, "deploy", &["x".to_string()]).unwrap_err();
    assert!(
        err.to_string().contains("build"),
        "names the offending beam: {err}"
    );
}

#[test]
fn test_arg_interpolation_leaves_shell_expansion_untouched() {
    let input = r#"beam "d" { run { commands = ["echo ${HOME} ${arg.1}"] } }"#;
    let mut bf = parse(input).unwrap();
    resolve_arguments(&mut bf, "d", &["x".to_string()]).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["echo ${HOME} x"]
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p aurora-core --test parser_test test_positional_arg_resolves_for_target`
Expected: FAIL — `resolve_arguments` is not defined and `Beam.args` does not exist (compile error).

- [ ] **Step 3: Add the `args` field to `Beam`**

In `crates/aurora-core/src/ast.rs`, add to the `Beam` struct (after the `variables` field from Task 1):

```rust
    /// Positional arguments passed to this beam on the command line. Only ever
    /// non-empty on the explicitly invoked target; folded into its cache key.
    pub args: Vec<String>,
```

- [ ] **Step 4: Initialise `args` in the two `Beam` literals**

In `crates/aurora-core/src/parser/mod.rs` (`parse_beam_block`) add `args: vec![],` to the `Beam { ... }` initialiser. In `crates/aurora/src/main.rs` add `args: vec![],` to the `__multi__` virtual beam literal.

- [ ] **Step 5: Extract the shared scanner and rewrite `interpolate_command` on it**

In `crates/aurora-core/src/parser/mod.rs`, replace the existing `interpolate_command` function with the shared scanner plus a thin variable resolver. Behaviour is unchanged (verified by the existing tests):

```rust
/// Scans `s` for `${...}` tokens and rewrites each via `resolve`. When
/// `resolve` returns `None` the token is copied verbatim (so `${HOME}` survives
/// for the shell); `Some(Err(_))` aborts. One scanner shared by the variable
/// and argument passes, so their `${...}` handling cannot drift apart.
fn interpolate_tokens(
    s: &str,
    resolve: impl Fn(&str) -> Option<Result<String>>,
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
                if let Some(result) = resolve(inner) {
                    out.push_str(&result?);
                    i = end + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// Interpolates `${var.<name>}` tokens in `s`. Non-`var` `${...}` sequences are
/// copied verbatim so shell parameter expansion still works. An unknown
/// variable is a hard error identified by `beam`.
fn interpolate_command(s: &str, vars: &HashMap<String, String>, beam: &str) -> Result<String> {
    interpolate_tokens(s, |inner| {
        let name = inner.strip_prefix("var.")?;
        if !is_ident(name) {
            return None;
        }
        Some(match vars.get(name) {
            Some(v) => Ok(v.clone()),
            None => Err(anyhow::anyhow!(
                "unknown variable '{}' referenced in beam '{}'",
                name,
                beam
            )),
        })
    })
}
```

- [ ] **Step 6: Add `resolve_arguments` and its helpers**

Append to `crates/aurora-core/src/parser/mod.rs`:

```rust
/// Interpolates `${arg.N}` and `${args}` in the invoked `target`'s run
/// commands, records the argument vector on that beam (so the cache key can
/// fold it in), and rejects `${arg...}` used in any other beam.
///
/// Arguments are target-only: a dependency is pulled in by the scheduler and
/// never receives invocation arguments, so referencing them there is a
/// configuration error rather than a silently empty expansion. Argument values
/// are inserted literally and never re-interpolated, so an argument containing
/// `${var.x}` or `${arg.1}` is not expanded a second time.
pub fn resolve_arguments(beam_file: &mut BeamFile, target: &str, args: &[String]) -> Result<()> {
    for beam in &mut beam_file.beams {
        let beam_name = beam.name.clone();
        if beam_name == target {
            if let Some(run) = &mut beam.run {
                for cmd in &mut run.commands {
                    *cmd = interpolate_arguments(cmd, args, &beam_name)?;
                }
            }
            beam.args = args.to_vec();
        } else if let Some(run) = &beam.run {
            for cmd in &run.commands {
                reject_arguments(cmd, &beam_name)?;
            }
        }
    }
    Ok(())
}

/// Interpolates `${args}` (whole tail, space-joined) and `${arg.N}` (1-based)
/// in a single command. Other `${...}` sequences are copied verbatim.
fn interpolate_arguments(s: &str, args: &[String], beam: &str) -> Result<String> {
    interpolate_tokens(s, |inner| {
        if inner == "args" {
            return Some(Ok(args.join(" ")));
        }
        let idx = inner.strip_prefix("arg.")?;
        Some(resolve_arg_index(idx, args, beam))
    })
}

/// Resolves a single `${arg.N}` reference (1-based) to its value, or a hard
/// error for a non-numeric index, a zero index, or an out-of-range index.
fn resolve_arg_index(idx: &str, args: &[String], beam: &str) -> Result<String> {
    let n: usize = idx.parse().map_err(|_| {
        anyhow::anyhow!("invalid argument reference '${{arg.{idx}}}' in beam '{beam}'")
    })?;
    if n == 0 {
        bail!("argument index is 1-based, got '${{arg.0}}' in beam '{beam}'");
    }
    args.get(n - 1).cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "missing argument '${{arg.{n}}}' in beam '{beam}': {} argument(s) provided",
            args.len()
        )
    })
}

/// Fails when a non-target beam references `${arg.N}` or `${args}`: arguments
/// are only available to the invoked target.
fn reject_arguments(s: &str, beam: &str) -> Result<()> {
    interpolate_tokens(s, |inner| {
        if inner == "args" || inner.strip_prefix("arg.").is_some() {
            Some(Err(anyhow::anyhow!(
                "beam '{beam}' references '${{{inner}}}', but arguments are only available to the invoked target"
            )))
        } else {
            None
        }
    })
    .map(|_| ())
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p aurora-core --test parser_test`
Expected: PASS (the new argument tests and every existing parser test, proving the scanner refactor changed no behaviour).

- [ ] **Step 8: Commit**

```bash
git add crates/aurora-core/src/ast.rs crates/aurora-core/src/parser/mod.rs crates/aurora/src/main.rs crates/aurora-core/tests/parser_test.rs
git commit -m ":sparkles: feat(parser): interpolate positional beam arguments"
```

---

## Task 3: Fold arguments into the invoked target's cache key

**Files:**
- Modify: `crates/aurora-core/src/cache.rs` (add `hash_with_args`)
- Modify: `crates/aurora-core/src/scheduler.rs` (`cache_lookup_blocking` signature + call site)
- Test: `crates/aurora-core/tests/cache_test.rs`

**Interfaces:**
- Consumes: `Beam.args` from Task 2; `BeamCache::hash_inputs_at` (unchanged).
- Produces: `pub fn BeamCache::hash_with_args(inputs_hash: &str, args: &[String]) -> String` — returns `inputs_hash` unchanged when `args` is empty (so every non-target beam keeps its existing key), otherwise a hash combining both.

- [ ] **Step 1: Write the failing test**

Add to `crates/aurora-core/tests/cache_test.rs`:

```rust
#[test]
fn test_hash_with_args_empty_is_identity() {
    assert_eq!(BeamCache::hash_with_args("abc123", &[]), "abc123");
}

#[test]
fn test_hash_with_args_differs_by_arguments() {
    let a = BeamCache::hash_with_args("abc123", &["web-01".to_string()]);
    let b = BeamCache::hash_with_args("abc123", &["web-02".to_string()]);
    assert_ne!(a, b, "different arguments must produce different keys");
    assert_ne!(a, "abc123", "arguments must change the key");
}

#[test]
fn test_hash_with_args_is_stable_and_order_sensitive() {
    let args1 = vec!["a".to_string(), "b".to_string()];
    let args2 = vec!["a".to_string(), "b".to_string()];
    let reordered = vec!["b".to_string(), "a".to_string()];
    assert_eq!(
        BeamCache::hash_with_args("h", &args1),
        BeamCache::hash_with_args("h", &args2),
        "same arguments hash the same"
    );
    assert_ne!(
        BeamCache::hash_with_args("h", &args1),
        BeamCache::hash_with_args("h", &reordered),
        "argument order matters"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p aurora-core --test cache_test test_hash_with_args_empty_is_identity`
Expected: FAIL — `hash_with_args` is not defined.

- [ ] **Step 3: Implement `hash_with_args`**

In `crates/aurora-core/src/cache.rs`, add this associated function inside `impl BeamCache`:

```rust
    /// Folds an argument vector into an inputs hash so the invoked target
    /// re-runs when its arguments change even though its `inputs` files did
    /// not. An empty vector returns the hash unchanged, so every beam without
    /// arguments (all but the invoked target) keeps its existing cache key.
    pub fn hash_with_args(inputs_hash: &str, args: &[String]) -> String {
        if args.is_empty() {
            return inputs_hash.to_string();
        }
        let mut hasher = Sha256::new();
        hasher.update(inputs_hash.as_bytes());
        for arg in args {
            hasher.update(b"\0");
            hasher.update(arg.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
```

- [ ] **Step 4: Run the cache tests to verify they pass**

Run: `cargo test -p aurora-core --test cache_test`
Expected: PASS (new tests plus all existing cache tests, whose `hash_inputs_at` calls are untouched).

- [ ] **Step 5: Thread `beam.args` into the scheduler's cache lookup**

In `crates/aurora-core/src/scheduler.rs`, change `cache_lookup_blocking` to accept the arguments and salt the hash. Update its signature and body:

```rust
async fn cache_lookup_blocking(
    cache: &Arc<BeamCache>,
    beam_name: &str,
    inputs: &[String],
    outputs: &[String],
    args: &[String],
    working_dir: &Path,
) -> CacheLookup {
    let cache = cache.clone();
    let beam_name = beam_name.to_string();
    let inputs = inputs.to_vec();
    let outputs = outputs.to_vec();
    let args = args.to_vec();
    let working_dir = working_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let hash = cache
            .hash_inputs_at(&working_dir, &inputs)
            .ok()
            .flatten()
            .map(|h| BeamCache::hash_with_args(&h, &args));
        if let Some(ref hash) = hash {
            if cache.is_valid(&beam_name, hash, &outputs, &working_dir) {
                let (stdout, stderr) = cache.load_logs(&beam_name);
                return CacheLookup::Hit { stdout, stderr };
            }
        }
        CacheLookup::Miss { hash }
    })
    .await
    .unwrap_or(CacheLookup::Miss { hash: None })
}
```

Then update the single call site in `run_beam_task` to pass `&beam.args`:

```rust
        match cache_lookup_blocking(
            &cache,
            &beam.name,
            &beam.inputs,
            &beam.outputs,
            &beam.args,
            &working_dir,
        )
        .await
```

- [ ] **Step 6: Run the full core suite to verify nothing regressed**

Run: `cargo test -p aurora-core`
Expected: PASS (the scheduler still compiles and every scheduler/cache test passes; non-target beams carry empty `args`, so their keys are unchanged).

- [ ] **Step 7: Commit**

```bash
git add crates/aurora-core/src/cache.rs crates/aurora-core/src/scheduler.rs crates/aurora-core/tests/cache_test.rs
git commit -m ":sparkles: feat(cache): fold beam arguments into the target cache key"
```

---

## Task 4: CLI wiring — capture arguments and interpolate end to end

**Files:**
- Modify: `crates/aurora/src/main.rs` (clap positional; call `resolve_arguments`)
- Test: `crates/aurora/tests/args_cli_test.rs` (new)

**Interfaces:**
- Consumes: `aurora_core::parser::resolve_arguments` from Task 2.
- Produces: the running behaviour — `aurora <target> [ARG]...` binds arguments to the target; `--` forwards a hyphen-leading tail; Aurora's own flags precede the target.

- [ ] **Step 1: Write the failing test**

Create `crates/aurora/tests/args_cli_test.rs`:

```rust
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn fixture_dir(beamfile: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Beamfile"), beamfile).unwrap();
    dir
}

const BEAMFILE: &str = r#"
aurora { version = "1"  default = "greet" }

beam "greet" {
  run { commands = ["echo hello ${arg.1}"] }
}

beam "passthrough" {
  run { commands = ["echo got:${args}"] }
}

beam "needy" {
  run { commands = ["echo ${arg.1}"] }
}
"#;

#[test]
fn positional_argument_reaches_the_target() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["greet", "Alice", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "run failed:\n{stdout}");
    assert!(stdout.contains("hello Alice"), "argument not applied:\n{stdout}");
}

#[test]
fn double_dash_forwards_a_hyphen_leading_tail() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["passthrough", "--no-tui", "--", "--flag", "value"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "run failed:\n{stdout}");
    assert!(stdout.contains("got:--flag value"), "tail not forwarded:\n{stdout}");
}

#[test]
fn missing_argument_fails_before_running() {
    let dir = fixture_dir(BEAMFILE);
    let output = Command::new(env!("CARGO_BIN_EXE_aurora"))
        .args(["needy", "--no-tui"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_ne!(output.status.code(), Some(0), "must fail on a missing argument");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing argument"),
        "error must name the missing argument:\n{stderr}"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p aurora --test args_cli_test positional_argument_reaches_the_target`
Expected: FAIL — arguments are not captured, so `${arg.1}` stays literal and `echo hello ${arg.1}` does not contain `hello Alice` (or the run errors on the unresolved token).

- [ ] **Step 3: Capture positional arguments in clap**

In `crates/aurora/src/main.rs`, add an `args` positional after the existing `beam` argument (index 2, variadic). Aurora's own flags keep their meaning; a `--` routes a hyphen-leading tail into this positional:

```rust
        .arg(
            Arg::new("args")
                .help("Positional arguments for the target beam (use -- before hyphen-leading values)")
                .index(2)
                .num_args(0..),
        )
```

- [ ] **Step 4: Interpolate arguments once the target is known**

In `crates/aurora/src/main.rs`, immediately after the `let target = ...` block (the `if interactive { ... } else { ... }` that resolves the target, before the executor registration), collect the arguments and resolve them:

```rust
    // Positional arguments belong to the explicitly invoked target. Resolve
    // `${arg.N}` / `${args}` now that the target is known; a value that must
    // reach a dependency is a global variable, not an argument.
    let args: Vec<String> = matches
        .get_many::<String>("args")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    aurora_core::parser::resolve_arguments(&mut beam_file, &target, &args)?;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p aurora --test args_cli_test`
Expected: PASS (all three tests).

- [ ] **Step 6: Run the whole workspace suite**

Run: `cargo test --workspace`
Expected: PASS.

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/aurora/src/main.rs crates/aurora/tests/args_cli_test.rs
git commit -m ":sparkles: feat(cli): pass positional arguments to the target beam"
```

---

## Task 5: Documentation

**Files:**
- Modify: `ROADMAP.md` (mark the item shipped)
- Modify: `README.md` (document arguments and beam-local variables)

**Interfaces:**
- Consumes: the shipped feature.
- Produces: user-facing documentation.

- [ ] **Step 1: Mark the ROADMAP item shipped**

In `ROADMAP.md`, under "Table stakes", change the per-invocation arguments item from `- [ ]` to `- [x]` and tighten its wording to describe what shipped:

```markdown
- [x] **Per-invocation beam arguments** — `aurora deploy web-01` binds `web-01`
  to the invoked target as `${arg.1}`; `${args}` forwards the whole tail
  (`aurora test -- --nocapture`). Beams may also declare private, local
  `variable` blocks; a global `variable` remains the channel for values shared
  down the dependency chain.
```

- [ ] **Step 2: Document the feature in the README**

In `README.md`, add a subsection (near the existing variable/interpolation documentation) covering:
- positional `${arg.N}` (1-indexed) and `${args}` (whole tail), with the `aurora deploy web-01` and `aurora test -- --nocapture` examples;
- that arguments reach the invoked target only, and a missing `${arg.N}` is a hard error;
- beam-local `variable` blocks: private to the beam, shadow a global of the same name, not settable via `--var`; a global variable is how you share a value across a dependency chain.

Reuse the worked example from the spec (`docs/superpowers/specs/2026-07-02-per-invocation-beam-args-design.md`, "Worked examples").

- [ ] **Step 3: Verify the docs build/read cleanly**

Run: `cargo build --workspace`
Expected: builds clean (sanity check; docs are Markdown).

Manually re-read both edited sections for accuracy against the shipped behaviour.

- [ ] **Step 4: Commit**

```bash
git add ROADMAP.md README.md
git commit -m ":memo: docs(args): document per-invocation arguments and local variables"
```

---

## Self-Review

**1. Spec coverage:**
- Beam-local `variable` blocks (grammar, AST, scoping, `--var` encapsulation) → Task 1.
- `${arg.N}` / `${args}` interpolation, 1-indexing, missing-index error, literal insertion, target-only rejection, shared scanner → Task 2.
- Arguments folded into the target's cache key (target-only, non-target keys unchanged) → Task 3.
- CLI capture, `--` passthrough, flags-precede-target, resolve after target known → Task 4.
- ROADMAP + README documentation → Task 5.
- Interpolation-order (variables at parse, arguments post-CLI): variables resolve in Task 1's `resolve_variables` (parse time via `main.rs:81`), arguments resolve in Task 4's `resolve_arguments` call after target resolution. Literal insertion means order is safe. Covered.
- Out-of-scope items (named parameters, `dir`/`inputs`/`outputs`/gate interpolation of args, edge-wiring) are intentionally not implemented — consistent with the spec.

**2. Placeholder scan:** No "TBD"/"add error handling"/"similar to Task N". Every code step shows complete code; the only prose step is the README wording (Task 5, Step 2), which is documentation content, not code.

**3. Type consistency:**
- `Beam.variables: Vec<Variable>` (Task 1) and `Beam.args: Vec<String>` (Task 2) — both initialised in `parse_beam_block` and the `__multi__` literal.
- `resolve_arguments(&mut BeamFile, &str, &[String]) -> Result<()>` — defined Task 2, called Task 4 with `(&mut beam_file, &target, &args)`. Matches.
- `BeamCache::hash_with_args(&str, &[String]) -> String` — defined Task 3, called in `cache_lookup_blocking` in the same task. Matches.
- `interpolate_tokens` / `interpolate_command` / `interpolate_arguments` / `resolve_arg_index` / `reject_arguments` — all defined in Task 2; `interpolate_command` keeps its existing `(&str, &HashMap, &str) -> Result<String>` signature so `resolve_variables` (Task 1) needs no change to its call sites.

**Note for the executor:** clap's plain variadic positional (not `trailing_var_arg`) is deliberate — it keeps Aurora's own flags meaningful when they appear after the target, and relies on `--` for hyphen-leading argument tails, exactly as the spec's CLI section describes. If a future need arises to place hyphen args without `--`, revisit `trailing_var_arg`, accepting that it would then capture flags placed after the target.
