# Beamfile DSL reference

A `Beamfile` is a sequence of top-level blocks: one optional `aurora` block, any number of `variable` blocks, one optional `environment` block, and any number of `beam` blocks. Comments start with `#` and run to end of line. Strings use double quotes. Lists are `["a", "b"]`.

## `aurora` block

Global settings.

```hcl
aurora {
  version         = "1"      # Beamfile format version
  default         = "check"  # beam to run when none is given
  max_parallelism = 8        # cap on concurrently running beams
}
```

All three fields are optional.

## `variable` blocks

Declare a variable with a default and an optional description. Overridable with `--var name=value`.

```hcl
variable "profile" {
  default     = "debug"
  description = "Cargo build profile"
}
```

A `variable` block may also be declared **inside a beam**. It is then local to
that beam: it shadows a top-level variable of the same name and is not reachable
by `--var` (which targets top-level variables only). Use a top-level variable
for a value shared across the dependency chain, a beam-local one for a value
private to a single beam.

```hcl
beam "deploy" {
  variable "strategy" { default = "rolling" }   # private to this beam
  run { commands = ["deploy.sh --strategy ${var.strategy}"] }
}
```

## `environment` block

Evaluated sequentially; each entry is visible to later entries. A value is either a string literal or a `shell(...)` call whose stdout becomes the value. Only an allowlist of the process environment is inherited, so declare what beams need here.

```hcl
environment {
  GIT_SHA = shell("git rev-parse --short HEAD")
  TAG     = "v1"
}
```

## `beam` blocks

A named task. All fields are optional except that a beam usually has a `run` block (a beam with only `depends_on` acts as an aggregate).

```hcl
beam "test" {
  description   = "Run the test suite"
  depends_on    = ["build"]          # beams that must succeed first
  inputs        = ["src/**", "Cargo.toml"]  # cache key (file contents + paths)
  outputs       = ["target/debug/app"]      # must exist on disk for a cache hit
  skip_if       = "test -f .skip-tests"      # shell command; the beam is skipped when this command exits zero (succeeds)
  allow_failure = false              # when true, a failure counts as success for scheduling

  run {
    commands = ["cargo test --workspace"]
  }
}
```

### `condition` block

The `condition {}` block is evaluated at runtime, before the beam runs: `any` succeeds if at least one clause exits
zero, `all` requires every clause to exit zero. When the condition is not met the beam is skipped. `skip_if` is the
single-command shorthand and is evaluated first.

```hcl
beam "deploy" {
  condition {
    all = [
      { shell = "test \"$BRANCH\" = main" },
      { shell = "test -f build/artifact" },
    ]
  }
  run { commands = ["./deploy.sh"] }
}
```

### `run` block

Holds the commands and, optionally, a named executor with its configuration. Executor fields accept string literals or `var.<name>` references.

```hcl
beam "build-in-container" {
  run {
    commands = ["cargo build --release"]
    executor "docker" {
      image   = "rust:1.79"
      workdir = "/app"
      profile = var.profile
    }
  }
}
```

When no `executor` is given, the `local` executor (native shell) is used.

Inside `commands`, `${var.<name>}` is replaced by the value of the Beamfile variable `<name>` (honouring `--var`
overrides). Any other `${...}` is passed through to the shell unchanged, so `${HOME}` and environment variables from the
`environment {}` block still expand normally. Referencing an undeclared variable is an error.

### Positional arguments

The invoked target can also receive positional arguments from the command line (`aurora deploy web-01`). Inside its
`commands`:

- `${arg.N}` — the Nth argument, 1-indexed (`${arg.1}`, `${arg.2}`, ...). Referencing an index beyond the arguments
  passed is an error.
- `${args}` — every argument joined by a single space; the empty string when none are passed. Handy for a passthrough
  tail (`aurora test -- --nocapture`).

Arguments are substituted in the invoked target only. A beam pulled in as a dependency never receives them; referencing
`${arg...}` in a beam that runs as a dependency is an error (use a top-level variable for a value shared down the
chain). Argument values are inserted literally and never re-interpolated, and a changed argument list re-runs the
target even when its `inputs` are unchanged.

```hcl
beam "deploy" {
  run { commands = ["deploy.sh --to ${arg.1}"] }
}

beam "test" {
  run { commands = ["cargo test ${args}"] }
}
```

## Worked example

```hcl
aurora {
  version = "1"
  default = "check"
}

variable "profile" {
  default = "debug"
}

environment {
  GIT_SHA = shell("git rev-parse --short HEAD")
}

beam "fmt" {
  description = "Format code"
  run { commands = ["cargo fmt --all"] }
}

beam "test" {
  description = "Run tests"
  depends_on  = ["fmt"]
  inputs      = ["src", "Cargo.toml"]
  run { commands = ["cargo test --workspace"] }
}

beam "check" {
  description = "Format + test"
  depends_on  = ["fmt", "test"]
}
```
