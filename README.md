# Aurora

[![Version](https://img.shields.io/github/v/release/jdevelop-io/aurora?color=blue)](https://github.com/jdevelop-io/aurora/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg?logo=rust)](https://www.rust-lang.org/)

Aurora is a task runner and build tool written in Rust, designed as an alternative to `make`, `just` and `taskfile`. Tasks, called **beams**, are described in a `Beamfile` using an HCL-inspired syntax. Aurora resolves their dependencies as a directed acyclic graph, runs them in parallel, and ships a TUI to follow execution in real time.

## Features

- **Beamfile DSL**: declarative, HCL-inspired syntax to describe beams, variables, params, environment and dependencies.
- **Parallel execution**: DAG-based scheduling (topological sort, cycle detection) backed by a tokio task pool, bounded and on by default.
- **Parameterized beams**: a `param` turns a beam into a template; distinct CLI arguments or dependency bindings each produce their own instance, run and cached independently.
- **Caching**: SHA-256 hashing of a beam's `inputs` *and* of its definition (commands, executor and its settings, `dir`, declared environment, param bindings); a beam is skipped only when all of them are unchanged and its outputs are present. Change a command without touching its inputs and Aurora re-runs, where `make` and `task` both hand back a stale result ([benchmarks](benchmarks/)).
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
aurora --json          # stream NDJSON events on stdout instead of plain logs
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

#### Machine-readable output (`--json`)

`--json` replaces the plain prefixed logs with newline-delimited JSON
(NDJSON) on stdout: one JSON object per line, each carrying `"schema": 1` so
the format can evolve without breaking consumers. No colour is ever emitted
in this mode. `--json` forces non-interactive mode and conflicts with
`-i`/`--interactive`, `--list` and `--dry-run`. Exit codes are unchanged
(`0` success, `1` failure, `130` on interrupt).

Every command's stdout/stderr output is carried as `beam_output` events;
nothing else is written to stdout as raw text. The event types are:

- `run_started`: `target`, `beams` (the resolved dependency closure), `at`.
- `beam_started`: `beam`, `at`.
- `beam_output`: `beam`, `stream` (`stdout` or `stderr`), `line`.
- `beam_completed`: `beam`, `status`, fields specific to that status, `at`,
  and `duration_ms` when the beam actually ran. `status` is one of:
  - `success`: plus `cached` (bool); `duration_ms` is present only when the
    beam actually ran (absent when `cached` is `true`).
  - `skipped`: plus `reason` (`cached`, `skip_if` or `condition_not_met`);
    no `duration_ms`.
  - `failed` / `failed_allowed`: plus `exit_code` and `duration_ms`.
  - `cancelled`: no extra fields and no `duration_ms`.
- `run_completed`: `success`, `duration_ms`, `at`.
- `error`: `kind` (`beamfile`, `variable`, `target`, `argument` or
  `internal`) and `message`, emitted for a pre-run failure (an invalid
  Beamfile, a dependency cycle, an unknown target, an unknown `--var` key, or
  a failing `environment {}` block).

`at` is an RFC 3339 UTC timestamp with millisecond precision, for example
`2026-07-17T10:00:00.120Z`.

A pre-run failure emits an `error` event on stdout and exits `1`. Depending on
when the failure is detected, this `error` may appear on its own (a parse
error, an unknown target, a bad `--var` key, a failing `environment {}`
block) or after a `run_started` (a dependency cycle, detected once the run
has begun). In neither case is it followed by a `run_completed`, so a
consumer must not assume every run closes with one.

```bash
aurora check --json | jq -r 'select(.event=="beam_completed") | "\(.beam) \(.status)"'
```

### Watch mode

Run a beam and re-run it whenever its inputs change:

```bash
aurora build --watch     # or -w
```

Aurora watches the `inputs` globs of the target's dependency closure plus the
Beamfile itself. On a change it runs a fresh cycle; the cache skips every beam
whose inputs and definition are unchanged. Editing the Beamfile re-parses and
re-runs. When no beam in the closure declares `inputs`, Aurora warns and watches
the Beamfile only.

Watch mode works headless and in the TUI. In the TUI, press `w` to toggle
watching on the current target; `-w` presets it on at launch. Leaving watch mode
(Ctrl-C headless, `q` in the TUI) exits with code 0: the interruption is the
normal way out, not a failure. Beam failures during cycles do not change the
exit code. `--watch` cannot be combined with `--json`, `--list`, or `--dry-run`.

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
- `depends_on`: list of prerequisite beams (the DAG), each either a bare beam name or an object binding the dependency's params (see below),
- `inputs` / `outputs`: files used for SHA-256 caching (the beam's own definition, including its resolved param bindings, is part of the key too, so editing a command, overriding a variable, or invoking the beam with different param values re-runs it),
- `param`: a declared parameter of the beam's own signature (see below),
- `skip_if` or `condition { any/all }`: execution conditions,
- `run { commands = [...] }`: commands to run, with an optional `executor` block (`local`, `docker`, plugin),
- `environment {}`: a per-beam overlay of the process environment, scoped to that one beam (see below),
- a beam without `run` is a pure orchestration aggregate.

Aurora has three distinct configuration concepts, each with its own scope:

- `variable {}` (top-level): file-wide configuration, overridable from outside the Beamfile with `--var name=value`, and referenced anywhere as `${var.name}`.
- `param` (per beam): the beam's own signature. A param supplies a value per invocation (CLI arguments) or per dependency edge, instead of per file, and is referenced only inside that beam as `${param.name}`. See the next section.
- `environment {}` (top-level and, optionally, per beam): the process environment made available to a beam's commands. The top-level block is evaluated once, sequentially, before any beam runs; a beam's own `environment {}` block is an overlay evaluated once per instance, and its values shadow the top-level ones for that beam only.

Inside a beam's `commands`, `dir`, `skip_if`, `condition` clauses and executor config, `${var.name}` is interpolated with the variable's value (after any `--var` override) and `${param.name}` with the instance's bound value; other `${...}` sequences are left for the shell. A beam's own `environment {}` values interpolate `${param.name}` the same way; they do not interpolate `${var.name}` (a literal value is used as-is, and a `shell(...)` command sees previously evaluated environment variables as real environment variables, by name, not as `${...}` tokens).

### Params: beam signatures, CLI arguments and instantiation

A `param` turns a beam into a template: instead of one fixed unit of work, the beam becomes a signature that can be invoked, or depended on, with different values. Each distinct set of bound values produces its own **instance**, with its own identity, its own run, and its own cache entry.

```hcl
beam "build" {
  param "version" {}
  inputs = ["src/**/*.rs"]
  run { commands = ["cargo build --release"] }
}

beam "deploy" {
  param "version" { description = "Version to deploy" }
  param "env"     { default = "staging" }

  depends_on = [
    { beam = "build", params = { version = "${param.version}" } }
  ]

  environment {
    DEPLOY_TARGET = "${param.env}"
  }

  run { commands = ["./deploy.sh ${param.version}"] }
}
```

`param "version" {}` declares a required parameter (no `default`, so it must be bound at invocation time); `param "env" { default = "staging" }` declares an optional one. `description` is shown in `--list` and in binding-error messages, and has no other effect.

**CLI arguments.** When a parameterized beam is the invoked target, its declared params are bound from the command line, positional-then-named: an argument containing `=` binds that named param directly (`env=production`), and every other argument fills the remaining unbound params in declaration order. A param left unbound after that falls back to its `default`; a required param left unbound, or a surplus positional argument, is a hard error naming the beam's signature.

```bash
aurora deploy 1.2.3
aurora deploy version=1.2.3 env=production
```

Both bind `version` to `1.2.3`; the first leaves `env` at its `staging` default, the second overrides it to `production`.

**Instantiation.** Binding a beam's params produces an instance, identified as `name[k=v,...]` (bindings sorted by key), or just `name` when it has no params. Two invocations with different bindings are two different instances: they run, cache and appear in the TUI independently. `--list` shows the signature, not an instance:

```
$ aurora --list
Available beams:
  build <version>
  deploy <version> [env=staging]
```

`--dry-run` shows the actual instance ids that would run:

```
$ aurora --dry-run deploy 1.2.3
Execution plan for 'deploy[env=staging,version=1.2.3]':
  level 0: build[version=1.2.3]
  level 1: deploy[env=staging,version=1.2.3]
```

`build[version=1.2]` and `build[version=1.3]` (from two separate invocations, or two dependents binding different values) are unrelated instances: each hashes, runs and is cached on its own.

**Edge bindings.** A `depends_on` entry can be a bare string (`"fmt"`) or an object binding the dependency's params explicitly: `{ beam = "build", params = { version = "${param.version}" } }`. There is no implicit forwarding: a dependency's param is bound only by an explicit entry in that `params` map (interpolated in the parent's own bindings) or by its own `default`; anything else is a hard error at expansion time, naming the missing param and the exact object form to add.

### Migrating from positional arguments and beam-local variables

Two mechanisms from earlier versions of Aurora are gone:

- `${arg.N}` and `${args}` (positional CLI arguments, forwarded verbatim into the invoked target's commands) are removed. Declare a `param` instead and reference `${param.name}`. A Beamfile still referencing `${arg.N}` or `${args}` fails to parse, with a migration error pointing at the offending beam.
- a beam-local `variable {}` block (private to one beam, shadowing a global variable of the same name) is removed. Declare a `param` with a `default` instead: it plays the same role, a value private to the beam with a fallback, but is also overridable per invocation and per dependency edge, which a local variable never was. A beam still declaring its own `variable {}` block fails to parse, with a migration error.

A top-level `variable {}` (overridable with `--var`) is unaffected: it keeps propagating to every beam that references it, including across dependency edges.

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
