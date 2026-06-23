# Aurora

Aurora is a task runner and build tool written in Rust, designed as an alternative to `make`, `just` and `taskfile`. Tasks, called **beams**, are described in a `Beamfile` using an HCL-inspired syntax. Aurora resolves their dependencies as a directed acyclic graph, runs them in parallel, and ships a TUI to follow execution in real time.

## Features

- **Beamfile DSL**: declarative, HCL-inspired syntax to describe beams, variables, environment and dependencies.
- **Parallel execution**: DAG-based scheduling (topological sort, cycle detection) backed by a tokio task pool.
- **Caching**: SHA-256 hashing of `inputs`; a beam is skipped when its inputs are unchanged and its outputs are present.
- **Executors**:
  - `local`: native shell execution (default),
  - `docker`: execution inside a container through the Docker CLI,
  - WASM plugins via [extism](https://extism.org/) for community executors.
- **TUI** (ratatui): beam picker with fuzzy search, execution view with spinners, log streaming, and per-beam rerun (`r`).

## Installation

The project builds with Cargo (recent stable edition):

```bash
git clone git@github.com:jdevelop-io/aurora.git
cd aurora
cargo build --release
```

The binary lands in `target/release/aurora`.

## Usage

```bash
aurora                 # launch the TUI (beam picker)
aurora <beam>          # run a beam (and its dependencies)
aurora --list          # list all available beams
aurora --dry-run       # show which beams would run, without running them
aurora --no-cache      # ignore the cache
aurora --var key=val   # override a Beamfile variable
```

With no argument, the `default` beam declared in the `aurora {}` block is used.

## The Beamfile

Minimal example (the one Aurora uses to build itself):

```hcl
aurora {
  version = "1"
  default = "check"
}

beam "fmt" {
  description = "Format Rust code"
  run { commands = ["cargo fmt --all"] }
}

beam "clippy" {
  description = "Lint with clippy"
  depends_on  = ["fmt"]
  run { commands = ["cargo clippy --workspace -- -D warnings"] }
}

beam "test" {
  description = "Run all tests"
  depends_on  = ["fmt"]
  run { commands = ["cargo test --workspace"] }
}

beam "check" {
  description = "Format + lint + test"
  depends_on  = ["clippy", "test"]
}
```

A beam can declare:

- `description`: text shown in the TUI and in `--list`,
- `depends_on`: list of prerequisite beams (the DAG),
- `inputs` / `outputs`: files used for SHA-256 caching,
- `skip_if` or `condition { any/all }`: execution conditions,
- `run { commands = [...] }`: commands to run, with an optional `executor` block (`local`, `docker`, plugin),
- a beam without `run` is a pure orchestration aggregate.

Variables are declared with `variable {}` (overridable via `--var`) and referenced with `var.name`. The `environment {}` block defines variables evaluated sequentially and available to every beam.

## Architecture

The project is a Cargo workspace split into crates:

| Crate                    | Role                                                        |
|--------------------------|-------------------------------------------------------------|
| `aurora`                 | CLI binary, entry point and WASM plugin loading             |
| `aurora-core`            | Beamfile parser, DAG engine, scheduler, cache               |
| `aurora-tui`             | ratatui interface (picker, execution view, logs)            |
| `aurora-executor-api`    | Shared trait and types between host and executors           |
| `aurora-executor-local`  | Local shell executor                                        |
| `aurora-executor-docker` | Docker executor                                             |

Design documents and implementation plans live in `docs/plans/`.

## Development

Aurora builds with Aurora (dogfooding). With Cargo directly:

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## Status

Project at v0.1, under active development. The main building blocks (parser, DAG, local and docker executors, cache, TUI) are in place. See `docs/plans/` for the roadmap.

## License

Released under the [MIT License](LICENSE).
