# Per-invocation beam arguments (`${arg.N}` / `${args}`) and beam-local variables

Status: draft, awaiting user approval before implementation.

## Motivation

Table-stakes ergonomic (ROADMAP: "Per-invocation beam arguments", "the
most-requested missing ergonomic"). Today the only way to pass a value into a
run is the global `--var key=val`, which overrides a `variable` default
everywhere. There is no way to say `aurora deploy staging` and hand `staging`
to just that target, and no way to forward a free-form tail of flags to a
command (`aurora test -- --nocapture -p aurora-core`). Both `just` and `task`
support per-recipe arguments; this closes that gap.

The feature has two motivating shapes:

- **positional**: `aurora deploy web-01` binds `web-01` to the invoked beam,
  read as `${arg.1}`;
- **passthrough**: `aurora test -- --nocapture` forwards the whole tail, read as
  `${args}`.

While designing this it became clear the argument model only makes sense next to
a clear variable-scoping story (what is shared across the dependency chain versus
private to one beam). This spec therefore also introduces **beam-local
`variable` blocks**, which round out the model. See "Decisions taken" for the
option to ship the two parts separately.

## The model in one table

The design deliberately keeps a single value concept (`variable`, read as
`${var.x}`) and adds positional arguments as a second, invocation-scoped input.

| Need                                             | Tool                     | Syntax                                  |
|--------------------------------------------------|--------------------------|-----------------------------------------|
| Value shared / propagated down the chain         | **global** `variable`    | `${var.x}`, settable with `--var x=…`   |
| Value private to one beam (constant, derived)    | **local** `variable`     | `${var.x}` inside the beam, shadows global |
| Positional input to the invoked target only      | **argument**             | `${arg.1}`, `${arg.2}` (1-indexed)      |
| The whole argument tail at once                  | **arguments**            | `${args}`                               |

The rule of thumb: *where you declare a `variable` is its scope*. Top-level is
global and propagates to dependencies; inside a beam is private and does not.
Anything that must be tunable from the CLI across a chain is a **global**
variable (distinct names for independent knobs). Positional `${arg.N}` belongs
to the one explicitly invoked target.

## DSL syntax

### Beam-local `variable` blocks

The existing `variable "name" { default = "…" description = "…" }` block becomes
legal **inside** a beam, with identical grammar, AST (`Variable`) and `${var.x}`
interpolation:

```hcl
variable "env" { default = "qa" }          # global: crosses the chain

beam "deploy" {
  variable "strategy" { default = "rolling" }  # local: private to deploy
  depends_on = ["build"]
  run { commands = ["deploy.sh --env ${var.env} --strategy ${var.strategy}"] }
}
```

### Positional arguments in commands

No grammar change: like `${var.x}`, argument tokens live inside command string
literals and are resolved during interpolation.

```hcl
beam "deploy" {
  run { commands = ["deploy.sh --to ${arg.1}"] }   # aurora deploy web-01
}

beam "test" {
  run { commands = ["cargo test ${args}"] }        # aurora test -- --nocapture -p aurora-core
}
```

- `${arg.N}` — the Nth positional argument, **1-indexed**.
- `${args}` — every positional argument joined by a single space.

### Command line

```
aurora [FLAGS] <target> [ARG]...
aurora [FLAGS] <target> -- [ARG]...
```

- Bare tokens after the target are its positional arguments
  (`aurora deploy web-01 canary`).
- `--` forces every following token into the argument list verbatim, so an
  argument may itself begin with `-` (`aurora test -- --nocapture`). This is the
  only way to pass hyphen-leading arguments, since otherwise clap would try to
  parse them as Aurora flags.
- Aurora's own flags (`--no-cache`, `--var`, `--dry-run`, …) keep their meaning
  and are not part of `${args}`.

## Semantics

### Variable scoping and resolution

Resolution of `${var.x}` inside a beam, highest priority first:

1. the beam's **local** `variable "x"` if present (it shadows any global of the
   same name);
2. otherwise the **global** `variable "x"`, with any `--var x=…` override
   applied;
3. otherwise a **hard error** ("unknown variable", identical to today).

Consequences, all falling out of ordinary lexical scoping:

- **Global variables propagate.** A dependency pulled into the run reads the same
  `${var.x}` and therefore the same value. This is the channel for anything that
  must cross the chain.
- **Local variables are private and encapsulated.** They are invisible to other
  beams and **not** reachable by `--var` (`--var` targets global variables only).
  Two beams may each declare a local `strategy`; they are independent variables
  that merely share a name.
- **To tune a dependency from the CLI**, use a global variable (distinct names
  for independent knobs). Local variables are for values never set from outside.

### Argument scoping

- **Target-only.** Only the explicitly invoked target receives arguments.
  Dependencies never see `${arg.N}` / `${args}`; a value that must reach a
  dependency is a global variable, not an argument.
- **1-indexed.** `${arg.1}` is the first argument. `${arg.0}` is an error.
- **Missing index is a hard error.** Referencing `${arg.2}` when one argument was
  passed fails with a clear message naming the beam and the index, consistent
  with the unknown-variable behaviour. `${args}` with no arguments is the empty
  string (a passthrough tail is legitimately optional).
- **Literal insertion.** An argument's text is inserted as-is and is **not**
  re-scanned for `${…}`. Passing `aurora deploy '${var.env}'` inserts the literal
  string, never a second interpolation. This mirrors how a resolved `${var.x}`
  value is not re-interpolated, and avoids an injection / recursion foot-gun.
- **Referencing arguments outside the target** is unsupported in v1: a non-target
  beam that contains `${arg…}` is rejected at resolve time with a clear error
  ("arguments are only available to the invoked target"), rather than leaking a
  literal `${arg.1}` into a shell.

### Interpolation order

Two phases, because the two inputs are known at different times:

1. **Variables** are resolved during parsing (after `--var` is applied), against
   each beam's effective map (local overlaid on global). This is today's
   `resolve_variables` pass, extended for local variables.
2. **Arguments** are resolved after the CLI is parsed, for the invoked target
   only, once its positional arguments are known.

Because argument values are inserted literally, phase order is irrelevant to
correctness: any `${var.x}` is already gone before phase 2, and an argument
containing `${…}` is never interpreted.

### Caching

The invoked target's cache key **folds in its ordered argument vector**, so a
different argument list re-runs it even when the declared `inputs` are unchanged.
Dependencies carry no arguments and their keys are unaffected. (The cache
currently hashes `inputs` file contents and paths; arguments are added as an
extra hashed component for the target beam only.)

## Worked examples

Global + local + positional in one beam:

```hcl
variable "env" { default = "qa" }        # global -> propagates

beam "build" {
  run { commands = ["build.sh --env ${var.env}"] }
}

beam "deploy" {
  depends_on = ["build"]
  variable "strategy" { default = "rolling" }   # local -> private
  run { commands = ["deploy.sh --env ${var.env} --to ${arg.1} --strategy ${var.strategy}"] }
}
```

| Command                                   | `arg.1`  | `var.env` (global) | `var.strategy` (local) |
|-------------------------------------------|----------|--------------------|------------------------|
| `aurora deploy web-01`                    | `web-01` | `qa` (default)     | `rolling` (default)    |
| `aurora deploy web-01 --var env=prod`     | `web-01` | `prod` (override)  | `rolling`              |

For `aurora deploy web-01 --var env=prod`:

```
build   -> build.sh --env prod                                  # global propagated
deploy  -> deploy.sh --env prod --to web-01 --strategy rolling
```

Sharing a value between `build` and `deploy` is a global variable:

```hcl
variable "strategy" { default = "rolling" }

beam "build"  { run { commands = ["build.sh  --strategy ${var.strategy}"] } }
beam "deploy" { depends_on = ["build"]
                run { commands = ["deploy.sh --strategy ${var.strategy}"] } }
# aurora deploy --var strategy=blue  ->  both sides see "blue"
```

Passthrough tail:

```hcl
beam "test" { run { commands = ["cargo test ${args}"] } }
# aurora test -- --nocapture -p aurora-core  ->  cargo test --nocapture -p aurora-core
```

## Implementation touchpoints

1. **Grammar** (`parser/aurora.pest`): add `variable_block` to the `beam_field`
   alternation so a `variable {}` block is legal inside a beam. No grammar change
   for arguments (they live inside string literals, like `${var.x}`).
2. **AST** (`ast.rs`): add `pub variables: Vec<Variable>` to `Beam`.
3. **Parser** (`parser/mod.rs`):
   - `parse_beam_block`: initialise `variables: vec![]`, handle
     `Rule::variable_block` by reusing `parse_variable_block` and pushing onto
     `beam.variables`.
   - `resolve_variables`: build the effective map per beam as global variables
     overlaid with the beam's locals (local shadows global), and interpolate that
     beam's `dir` / `run.commands` / executor config against it. `--var`
     continues to apply to top-level variables only, so locals stay private.
   - Extract the `${namespace.key}` scanning loop out of `interpolate_command`
     into a shared helper taking a resolver callback, so the same scanner serves
     both the variable pass and the new argument pass (keeps the two phases
     separate without duplicating the parser). Unknown-`var` behaviour unchanged.
4. **Argument resolution** (`parser/mod.rs` + `main.rs`): a new
   `resolve_arguments(beam, &args)` interpolates `${arg.N}` / `${args}` in the
   invoked target's `run.commands` using the shared scanner, with the missing-
   index and literal-insertion rules above. `main.rs` calls it on the CLI target
   after argument parsing, and rejects `${arg…}` found in any non-target beam.
5. **CLI** (`main.rs`, clap): accept a trailing `Vec<String>` of arguments after
   the target (`trailing_var_arg` + `--` support for hyphen-leading values).
6. **Cache** (`cache.rs` / scheduler): fold the target's ordered argument vector
   into its cache hash. Dependencies unchanged.
7. No change to `aurora-executor-api` or the executors: they receive already
   interpolated commands.

## Testing plan

Integration tests (per repo convention, in each crate's `tests/`):

- parser: a beam containing a `variable {}` block parses into
  `Beam.variables`; a local variable shadows a global of the same name; a local
  variable is not affected by a `--var` override of the same name.
- parser/resolve: `${arg.1}` and `${args}` resolve for the target; `${arg.2}`
  with one argument is a hard error; an argument value containing `${var.x}` is
  inserted literally (no second interpolation); `${arg…}` in a non-target beam is
  rejected.
- scheduler: `aurora deploy web-01` runs the target with `arg.1 = web-01` and
  leaves a dependency's commands argument-free; a global variable set via `--var`
  reaches a dependency, a local one does not.
- cache: two runs of the same target with different arguments produce a cache
  miss (re-run); identical arguments hit.
- CLI: `--` forwards hyphen-leading tokens into `${args}`; Aurora's own flags are
  not captured as arguments.

## Out of scope / decisions taken autonomously (please confirm on review)

- **Positional, index-based arguments** (`${arg.N}` / `${args}`) rather than
  declared/named parameters. Chosen for maximal consistency with the existing
  `${var.x}` interpolation and minimal new surface. Named/declared parameters
  with defaults and `--list` discoverability can be layered on later without
  conflict.
- **Bundling beam-local variables** into this spec. They emerged as the clean
  answer to "what is private versus shared" while designing arguments. If you
  prefer a leaner first change, we can split this into two items: (a) positional
  arguments alone (which already deliver the requested ergonomic on top of the
  existing global variables), then (b) beam-local variables as a follow-up.
- **Arguments interpolate in `run.commands` only** for v1, not in `dir`,
  `inputs`, `outputs`, or the gates. This matches the passthrough motivation and
  keeps the cache-structural fields free of invocation-time values. (Note gates
  are not variable-interpolated today either.)
- **`--var` remains global-only**; it does not reach beam-local variables. This
  is what makes local variables a real encapsulation boundary.
- **Explicit dependency-edge wiring** (`depends_on = ["build(strategy=…)"]`, a
  `just`-style way to let a beam drive a dependency's value) is deliberately
  **not** in this version; global variables cover the shared case. It remains a
  possible later addition.
