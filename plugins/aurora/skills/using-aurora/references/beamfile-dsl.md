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

Note: the `condition {}` block is currently parsed but not yet evaluated at runtime, so it has no effect today. For
conditional execution, use `skip_if`. The syntax is documented here for forward compatibility.

Run the beam only if shell clauses pass. `any` succeeds if at least one clause exits zero; `all` requires every clause to exit zero.

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
