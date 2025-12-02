# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with this codebase.

## Project Overview

Aurora is a next-generation task automation and build system written in Rust. It uses an HCL-inspired DSL called "Beamfile" to define build targets (called "beams") with dependencies, commands, conditions, and hooks.

## Architecture

The project is organized as a Cargo workspace with 5 crates:

- **crates/core** (`aurora-core`): Core types and traits (Beam, Beamfile, Variable, Condition, Hook, errors)
- **crates/parser** (`aurora-parser`): Beamfile DSL parser using nom 8 combinators
- **crates/engine** (`aurora-engine`): Execution engine with DAG resolution, scheduler, parallel executor, command runner, and build cache
- **crates/plugin** (`aurora-plugin`): WASM plugin system using wasmtime 39
- **crates/cli** (`aurora-cli`): Command-line interface using clap

### Key Design Decisions

- **Parser**: Uses `nom` 8 with `nom_locate` for position tracking in error messages
- **DAG**: Uses `petgraph` for dependency graph with cycle detection
- **Parallelism**: Uses `tokio` async runtime with semaphore-based concurrency control
- **Cache**: Uses `blake3` for fast file hashing to skip unchanged beams
- **Cross-platform**: Abstracts shell execution (bash/sh on Unix, PowerShell/cmd on Windows)
- **Plugins**: Uses `wasmtime` 39 for WebAssembly plugin execution with sandboxed capabilities

## Common Commands

```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run clippy lints
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt

# Build release binary
cargo build --release

# Run the CLI
./target/release/aurora --help

# Test aurora on itself (from project root after creating Beamfile)
./target/release/aurora list
./target/release/aurora validate
```

## Beamfile DSL Syntax

The parser expects this structure:

```hcl
variable "name" {
  default = "value"
  description = "Description"
}

beam "target" {
  description = "What this beam does"
  depends_on = ["other", "beams"]

  condition {
    file_exists = "path/to/file"
  }

  env {
    KEY = "value"
    # Variables can be interpolated
    MESSAGE = "Hello, ${var.name}!"
  }

  pre_hook {
    commands = ["echo 'before'"]
  }

  run {
    # Variable interpolation supported:
    # - ${var.name} - Beamfile variables
    # - ${env.NAME} - Environment variables
    # - ${beam.name} - Current beam name
    # - $$ - Literal dollar sign
    commands = ["echo Building ${beam.name}", "echo Version: ${var.version}"]
    shell = "bash"
    working_dir = "."
    fail_fast = true
  }

  post_hook {
    commands = ["echo 'after'"]
  }

  inputs = ["src/**/*.rs"]
  outputs = ["target/release/binary"]
}

default = "target"
```

## Code Style

- Use `rustfmt` with default settings
- Follow Rust API guidelines
- Prefer explicit error handling with `thiserror`
- Use `miette` for user-facing error messages with spans
- Keep functions small and focused
- Write tests for parsers and core logic

## Testing Strategy

- Unit tests in each module (`#[cfg(test)]` modules)
- Parser tests cover individual combinators and full Beamfile parsing
- Engine tests cover DAG operations, cache, and command execution
- Integration tests via CLI commands

## File Locations

- Main CLI entry: `crates/cli/src/main.rs`
- CLI commands: `crates/cli/src/commands/`
- Beamfile discovery: `crates/cli/src/discovery.rs`
- Parser combinators: `crates/parser/src/combinators.rs`
- AST types: `crates/parser/src/ast.rs`
- Core types: `crates/core/src/` (beam.rs, beamfile.rs, variable.rs, etc.)
- DAG implementation: `crates/engine/src/dag.rs`
- Parallel executor: `crates/engine/src/executor.rs`
- Build cache: `crates/engine/src/cache.rs`
- Variable interpolation: `crates/core/src/interpolation.rs`
- Plugin runtime: `crates/plugin/src/runtime.rs`
- Plugin manifest: `crates/plugin/src/manifest.rs`
- Plugin host functions: `crates/plugin/src/host.rs`

## Current Status

- **Phase 1 (Complete)**: Foundation - workspace, core types, parser, CLI, basic execution
- **Phase 2 (Complete)**: Enhanced parallel execution with tokio::spawn
  - True parallel beam execution using `tokio::spawn`
  - Thread-safe `SharedReport` with `Mutex`/`RwLock`
  - Semaphore-based concurrency control
  - `BeamCallback` system for real-time event notifications
  - `OutputCallback` for streaming command output
  - `ExecutorBuilder` pattern for configuration
- **Phase 3 (Complete)**: Variable interpolation (`${var.name}`)
  - `InterpolationContext` for managing variable scope
  - `${var.name}` - Beamfile variable references
  - `${env.NAME}` - Environment variable references
  - `${beam.name}` - Current beam name reference
  - `${ctx.key}` - Extra context values
  - `$$` - Escaped literal dollar sign
  - Automatic interpolation of commands, env vars, and working_dir
- **Phase 4 (Complete)**: WASM plugin system with wasmtime 39
  - `PluginRuntime` for managing WASM plugins
  - `Plugin` and `PluginInstance` for loading and executing plugins
  - `PluginManifest` with JSON configuration (plugin.json)
  - `PluginCapabilities` for permission control (fs, network, env)
  - Host functions: `aurora_log`, `aurora_get_var`, `aurora_set_var`, `aurora_get_env`
  - Plugin exports: `plugin_name`, `plugin_version`, `on_beam_start`, `on_beam_complete`, `transform_command`
  - Memory management via `alloc`/`dealloc` exports
  - `PluginState` for variable and log management
- **Phase 5 (Pending)**: Watch mode, rich terminal UI, documentation
