# Aurora

[![Version](https://img.shields.io/github/v/release/jdevelop-io/aurora?color=blue)](https://github.com/jdevelop-io/aurora/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg?logo=rust)](https://www.rust-lang.org/)

Aurora is a task runner and build tool written in Rust, designed as an alternative to `make`, `just` and `taskfile`. Tasks, called **beams**, are described in a `Beamfile` using an HCL-inspired syntax. Aurora resolves their dependencies as a directed acyclic graph, runs them in parallel, and ships a TUI to follow execution in real time.

## Features

- **Beamfile DSL**: declarative, HCL-inspired syntax to describe beams, variables, environment and dependencies.
- **Parallel execution**: DAG-based scheduling (topological sort, cycle detection) backed by a tokio task pool, bounded and on by default.
- **Caching**: SHA-256 hashing of a beam's `inputs` *and* of its definition (commands, variables, executor and its settings, `dir`, declared environment); a beam is skipped only when all of them are unchanged and its outputs are present. Change a command without touching its inputs and Aurora re-runs, where `make` and `task` both hand back a stale result ([benchmarks](benchmarks/)).
- **Executors**:
  - `local`: native shell execution (default),
  - `docker`: execution inside a container through the Docker CLI,
  - WASM plugins via [extism](https://extism.org/) for community executors.
- **TUI** (ratatui): beam picker with fuzzy search, execution view with spinners, log streaming, and per-beam rerun (`r`).

## Installation

The quick-install scripts below download a prebuilt binary and need no build tools.
Docker is optional and only required at runtime for beams that use the `docker` executor.

### Homebrew (macOS and Linux)

```bash
brew install jdevelop-io/tap/aurora-runner
```

The formula is `aurora-runner`, but the installed binary is `aurora`: `homebrew/core`
already ships an unrelated `aurora` (a Beanstalkd queue console), so the shorter name
would hand you the wrong project. Completions and the man page are installed with it.

### Quick install (Linux and macOS)

Download the latest prebuilt binary for your platform and install it:

```bash
curl -fsSL https://raw.githubusercontent.com/jdevelop-io/aurora/main/install.sh | sh
```

By default the binary lands in `~/.local/bin`. Override the target directory or pin a
specific version with environment variables:

```bash
AURORA_INSTALL_DIR=/usr/local/bin AURORA_VERSION=v0.8.0 \
  curl -fsSL https://raw.githubusercontent.com/jdevelop-io/aurora/main/install.sh | sh
```

To update, just run the command again: it always fetches the latest release.

### Quick install (Windows)

In PowerShell, download the latest prebuilt binary and install it:

```powershell
irm https://raw.githubusercontent.com/jdevelop-io/aurora/main/install.ps1 | iex
```

The binary lands in `%LOCALAPPDATA%\aurora\bin`, which is added to your user `PATH`.
The same `AURORA_INSTALL_DIR` and `AURORA_VERSION` environment variables are supported.

### With cargo install

Requires a stable [Rust toolchain](https://rustup.rs/) (1.91 or later). To build the
binary from the latest source instead of downloading a prebuilt one:

```bash
cargo install --git https://github.com/jdevelop-io/aurora aurora-runner
```

The crate is named `aurora-runner` because `aurora` was already taken on crates.io;
the installed binary is still called `aurora`.

The binary is placed in `~/.cargo/bin`, so make sure that directory is on your `PATH`.
Add `--force` to update an existing install.

### From source

Requires a stable [Rust toolchain](https://rustup.rs/). Clone the repository and build
the release binary:

```bash
git clone git@github.com:jdevelop-io/aurora.git
cd aurora
cargo build --release
```

The binary lands in `target/release/aurora`. Copy it somewhere on your `PATH`, for example:

```bash
install -m 0755 target/release/aurora ~/.local/bin/aurora
```

### Verify

```bash
aurora --version
```

### Shell completions and man page

Aurora generates them itself, so no build step is needed. Each release also ships
them prebuilt, in the `aurora-<version>-completions.tar.gz` archive.

```bash
aurora --completions zsh > ~/.zfunc/_aurora     # bash, zsh, fish, powershell, elvish
aurora --man > /usr/local/share/man/man1/aurora.1
```

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

### Examples

Run the default beam (here `check`, which fans out to `clippy` and `test`):

```bash
aurora
```

Run a specific beam and its dependencies. Running `clippy` first runs `fmt`,
since `clippy` declares `depends_on = ["fmt"]`:

```bash
aurora clippy
```

List the available beams with their descriptions:

```bash
$ aurora --list
check    Format + lint + test
clippy   Lint with clippy
fmt      Format Rust code
test     Run all tests
```

Preview the execution plan without running anything. Beams already satisfied by
the cache are flagged as skipped:

```bash
$ aurora check --dry-run
Execution plan for 'check':
  level 0: fmt
  level 1: clippy, test
  level 2: check
```

Force a full rebuild by ignoring the cache:

```bash
aurora check --no-cache
```

Override a variable declared in the Beamfile (for instance a Docker image used
by a `docker` executor):

```bash
aurora qa --var docker_image=omega-tools:v2.0.0
```

Override several variables at once:

```bash
aurora qa --var docker_image=omega-tools:v2.0.0 --var profile=ci
```

### In the TUI

Launch Aurora without arguments to open the picker, then:

- type to fuzzy-search a beam, `Enter` to run it,
- during execution, `↑`/`↓` to navigate beams and `Enter` to open the streamed logs,
- `r` to rerun the focused beam (and its dependents),
- `q` to quit.

### Non-interactive (headless) mode

Aurora auto-detects whether to show the TUI: when standard output is a terminal
it opens the ratatui interface; when output is piped or redirected (scripts, CI)
it runs headless, streaming plain per-beam output and printing a final recap.

Two flags force the choice explicitly:

- `--no-tui` — force plain output, even in a terminal.
- `-i`, `--interactive` — force the TUI, even when output is not a terminal.

Headless output prefixes each line with its beam name (stdout and stderr are
kept separate) and ends with an ASCII recap:

```
[build] Compiling aurora-core v0.5.0
[test]  running 12 tests

[PASS] build  4.2s
[FAIL] test   exit 1 1.8s
Done: 1 ok, 1 failed
```

Exit code: `0` when every beam succeeds (beams marked `allow_failure` count as
success), `1` when any beam fails, which also covers a malformed Beamfile
(a dependency cycle, an unknown dependency, an unknown target beam or an unknown
`--var` key), and `130` when the run is interrupted. Ctrl-C (or a `SIGTERM`)
cancels the running beams and reaps their process subtrees rather than leaving
them behind.
In headless mode the target beam is taken from the `aurora { default = ... }`
block when no beam is given; the interactive picker is only available with a
TTY or `-i`. ANSI colour appears only when the target stream (stdout or
stderr) is itself a terminal and `NO_COLOR` is unset, so redirecting one
stream does not pollute it with colour codes.

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
- `inputs` / `outputs`: files used for SHA-256 caching (the beam's own definition is part of the key too, so editing a command or overriding a variable re-runs it),
- `skip_if` or `condition { any/all }`: execution conditions,
- `run { commands = [...] }`: commands to run, with an optional `executor` block (`local`, `docker`, plugin),
- a beam without `run` is a pure orchestration aggregate.

Variables are declared with `variable {}` (overridable via `--var`) and referenced with `var.name`. The `environment {}` block defines variables evaluated sequentially and available to every beam. Inside a beam's `commands`, `${var.name}` is interpolated with the variable's value (after any `--var` override); other `${...}` sequences are left for the shell.

### Positional arguments and beam-local variables

Beyond the global `--var`, the invoked target can also receive per-invocation
positional arguments, and any beam can declare `variable {}` blocks of its
own that stay private to it.

Positional arguments follow the target on the command line and are
interpolated into that target's `run.commands`:

- `${arg.N}`: the Nth argument, 1-indexed. Referencing an index beyond the
  number of arguments passed is a hard error.
- `${args}`: every argument joined by a single space; the empty string when
  none are passed.

```hcl
beam "deploy" {
  run { commands = ["deploy.sh --to ${arg.1}"] }
}

beam "test" {
  run { commands = ["cargo test ${args}"] }
}
```

```bash
aurora deploy web-01                          # arg.1 = "web-01"
aurora test -- --nocapture -p aurora-runner-core     # args = "--nocapture -p aurora-runner-core"
```

Aurora's own flags are parsed before the target, so a hyphen-leading argument
must follow `--` (otherwise it would be read as an Aurora flag); a plain
positional value such as `web-01` needs no `--`.

Arguments reach the invoked target only:

- a beam that belongs to the target's dependency graph (one that will
  actually run as part of this invocation) and references `${arg.N}` or
  `${args}` is a hard error, because a dependency never receives the
  invocation's arguments; share the value through a global `variable`
  instead;
- an independent beam elsewhere in the Beamfile that references `${arg.N}` or
  `${args}` is left untouched, since it is not part of the current run.

A beam can also declare its own `variable {}` block, private to that beam and
shadowing a global variable of the same name. Unlike a global variable, a
local one is not reachable with `--var` (which targets globals only), so a
value that must be shared down a dependency chain has to stay a global
variable:

```hcl
variable "env" { default = "qa" }        # global: propagates to dependencies

beam "build" {
  run { commands = ["build.sh --env ${var.env}"] }
}

beam "deploy" {
  depends_on = ["build"]
  variable "strategy" { default = "rolling" }   # local: private to deploy
  run { commands = ["deploy.sh --env ${var.env} --to ${arg.1} --strategy ${var.strategy}"] }
}
```

```bash
aurora deploy web-01 --var env=prod
# build  -> build.sh --env prod
# deploy -> deploy.sh --env prod --to web-01 --strategy rolling
```

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

## Development

Aurora builds with Aurora (dogfooding). With Cargo directly:

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## Claude Code plugin

Aurora ships a [Claude Code](https://claude.ai/code) plugin so the assistant understands Aurora and can read, write, and run `Beamfile`s in your project.

### Install the plugin

Add this repository as a plugin marketplace, then install the `aurora` plugin. Run both commands inside Claude Code:

```text
/plugin marketplace add jdevelop-io/aurora
/plugin install aurora
```

- `/plugin marketplace add jdevelop-io/aurora` registers this repository's marketplace (`.claude-plugin/marketplace.json`) as a source.
- `/plugin install aurora` installs the plugin published by that marketplace.

### Update or remove

```text
/plugin marketplace update aurora
/plugin uninstall aurora
```

- `/plugin marketplace update aurora` refreshes the marketplace metadata to pick up new plugin versions.
- `/plugin uninstall aurora` removes the plugin.

### What the plugin provides

- **Skill `using-aurora`** — Aurora's execution model, the Beamfile DSL, and the CLI, with reference files loaded on demand.
- **Agent `aurora-expert`** — authors `Beamfile`s and migrates `make`/`just`/`taskfile`/npm scripts to Aurora.
- **Hooks** — validate a `Beamfile` after it is edited (`aurora --dry-run`) and surface available beams at session start. Both no-op silently when the `aurora` binary is not installed.

The plugin assumes the `aurora` binary is installed separately (see [Installation](#installation) above). See [`claude-code-plugin`](claude-code-plugin) for details.

## Status

Project at v0.8.0, under active development. The main building blocks (parser, DAG, local and docker executors, cache, TUI) are in place.

## License

Released under the [MIT License](LICENSE).
