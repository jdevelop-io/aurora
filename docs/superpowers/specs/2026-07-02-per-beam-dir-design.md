# Per-beam working directory (`dir`)

Status: draft, awaiting user approval before implementation.

## Motivation

Table-stakes ergonomic (ROADMAP: "Per-beam working directory (`dir`)",
"Essential in monorepos"). Today the `local` executor runs every beam from the
Beamfile directory, and a beam's `inputs`/`outputs` are also resolved against
that same directory. In a monorepo where each package lives in its own
subdirectory, this forces every path to be written from the repository root:

```hcl
beam "build-api" {
  inputs  = ["packages/api/src/**"]
  outputs = ["packages/api/dist/**"]
  run { commands = ["cd packages/api && npm run build"] }
}
```

The `cd` prefix is repeated in every command, and every input/output path
carries the `packages/api/` boilerplate. A per-beam `dir` removes both.

## DSL syntax

A new optional beam field:

```hcl
beam "build-api" {
  dir     = "packages/api"
  inputs  = ["src/**"]      # resolved under packages/api
  outputs = ["dist/**"]     # resolved under packages/api
  run { commands = ["npm run build"] }  # cwd = packages/api
}
```

`dir` is a single string. It supports `${var.name}` interpolation, consistent
with `run.commands` (for example `dir = "packages/${var.pkg}"`).

## Semantics: `dir` is the beam's root ("Beam root, everything")

When a beam declares `dir`, that directory becomes the base for **everything the
beam does**:

- the `run` command working directory (`local` executor `current_dir`, docker
  mount source);
- `inputs` and `outputs` path resolution (and therefore the cache input hash and
  output-presence check);
- the gate commands (`skip_if` and `condition {}` clauses).

Rationale: the monorepo value is package-local paths. This is also the *smaller*
change, not the larger one: `working_dir` is a single value carried in
`scheduler.rs`'s `TaskEnv` and threaded to gates, the cache lookup, and
`ExecutionInput`. Overriding that one value per beam makes every downstream
consumer inherit the beam's directory with no per-consumer plumbing.

### Path resolution of `dir` itself

- A relative `dir` is resolved against the **Beamfile directory** (the
  scheduler's root `working_dir`).
- An absolute `dir` is used as-is.
- No artificial restriction on `..`. A Beamfile already runs arbitrary shell
  commands (which can `cd` anywhere), so `dir` is not a new security boundary;
  adding a traversal check here would be security theatre inconsistent with the
  rest of the tool.

### What `dir` does NOT change

- **Cache storage location.** The cache directory stays at
  `<repo-root>/.aurora/cache` (constructed once in `Scheduler::new` from the root
  `working_dir`). Only *input/output resolution* moves under `dir`; the on-disk
  cache store does not. Beam-name sanitisation for cache file stems is unchanged.
- **The `environment {}` block.** It is evaluated once, globally, against the
  root working directory (`main.rs` -> `env::evaluate`), before any beam runs.
  `dir` is per-beam and does not retroactively change global environment
  evaluation. (A beam that needs a package-local environment value can still
  compute it inside its `run` commands.)

### Missing directory

If a beam's resolved `dir` does not exist (or is not a directory), the beam
fails with a clear error naming the beam and the path, rather than surfacing a
raw `sh: cannot cd` or a confusing cache miss. This check runs at the start of
the beam's execution, before gates and cache lookup.

## Implementation touchpoints

1. **Grammar** (`parser/aurora.pest`): add `beam_dir = { "dir" ~ "=" ~ string }`
   and include it in `beam_field`.
2. **AST** (`ast.rs`): add `pub dir: Option<String>` to `Beam`.
3. **Parser** (`parser/mod.rs`):
   - `parse_beam_block`: initialise `dir: None`, handle `Rule::beam_dir` via
     `unquote`.
   - `resolve_variables`: interpolate `${var.name}` in `beam.dir` using the
     existing `interpolate_command` helper (so an unknown variable is the same
     hard error as in commands).
4. **Scheduler** (`scheduler.rs`): in `run_beam_task`, after destructuring
   `TaskEnv`, compute the per-beam working directory:
   `let working_dir = beam.dir.as_ref().map(|d| working_dir.join(d)).unwrap_or(working_dir);`
   (`Path::join` already treats an absolute `d` as replacing the base.) Add the
   missing-directory check right after, before gating. Everything downstream
   (`gate_skip_reason`, `cache_lookup_blocking`, `ExecutionInput.working_dir`)
   already reads this local `working_dir`, so no further plumbing is needed.
5. No change to `aurora-executor-api`, `aurora-executor-local`, or
   `aurora-executor-docker`: they already consume `input.working_dir`.

## Testing plan

Integration tests (per repo convention, in each crate's `tests/`):

- `aurora-core` parser test: a beam with `dir = "..."` parses into
  `Beam.dir == Some(...)`; `dir` with `${var.x}` resolves; unknown var errors.
- `aurora-core` scheduler test: a beam with `dir` set runs its command in that
  directory (assert via an output that reveals `pwd`), and its relative `inputs`
  are hashed under `dir` (cache hit/miss driven by a file inside `dir`).
- `aurora-core` scheduler test: a beam whose `dir` is missing fails with the
  clear error and does not corrupt sibling beams.
- Gate test: a `skip_if`/`condition` command is evaluated with cwd = `dir`.

## Out of scope / decisions taken autonomously (please confirm on review)

- **Scope = "Beam root (everything)"** rather than "run commands only". Chosen
  from the ROADMAP's monorepo framing and the smaller-diff argument above. This
  was the open question when you stepped away.
- Per-invocation beam arguments (`aurora deploy staging`) remain a separate,
  later ROADMAP item; not addressed here.
- No new `dir`-traversal sandboxing (see rationale above).
