# Aurora

A next-generation task automation and build system written in Rust.

Aurora is designed to be a "Make on steroids" - a modern approach to defining and executing build targets with a focus on clarity, flexibility, and automation.

## Features

- **Beamfile configuration**: Define your build targets and tasks declaratively in a simple, HCL-inspired DSL
- **Beams (targets)**: Each beam represents a task with dependencies, commands, conditions, and hooks
- **Dependency resolution**: Automatically manages task dependencies using a DAG (Directed Acyclic Graph)
- **Parallel execution**: Independent beams can be executed in parallel for maximum efficiency
- **Build cache**: Skip unchanged beams using blake3 file hashing
- **Cross-platform**: Works on Linux, macOS, and Windows
- **Extensibility**: Supports variables, conditional execution, pre/post hooks

## Installation

### From source

```bash
git clone https://github.com/jdevelop-io/aurora.git
cd aurora
cargo install --path crates/cli
```

### Using Cargo

```bash
cargo install aurora-cli
```

## Quick Start

Initialize a new Beamfile in your project:

```bash
aurora init
```

This creates a `Beamfile` with example targets. List available beams:

```bash
aurora list
```

Run a beam:

```bash
aurora build
```

## Beamfile Syntax

```hcl
# Variables
variable "mode" {
  default = "release"
  description = "Build mode (debug/release)"
}

# Beam definition
beam "build" {
  description = "Build the project"
  depends_on = ["clean", "lint"]

  condition {
    file_exists = "Cargo.toml"
  }

  env {
    RUST_BACKTRACE = "1"
  }

  pre_hook {
    commands = ["echo 'Starting build...'"]
  }

  run {
    commands = [
      "cargo build --${var.mode}",
      "echo 'Build complete!'"
    ]
    shell = "bash"
    fail_fast = true
  }

  post_hook {
    commands = ["echo 'Done!'"]
  }

  outputs = ["target/${var.mode}/myapp"]
}

beam "test" {
  depends_on = ["build"]
  run {
    commands = ["cargo test"]
  }
}

default = "build"
```

## CLI Commands

```bash
aurora                     # Run default beam
aurora <beam>              # Run specific beam
aurora run <beam>          # Explicit run command
aurora list                # List all beams
aurora list --detailed     # List with descriptions and dependencies
aurora graph [beam]        # Show dependency graph
aurora graph --format dot  # Output in DOT format for Graphviz
aurora validate            # Validate Beamfile syntax
aurora cache clean         # Clear build cache
aurora cache status        # Show cache status
aurora init                # Create a new Beamfile
aurora --dry-run <beam>    # Show what would be executed
aurora -j 4 <beam>         # Set max parallelism
aurora --no-cache <beam>   # Disable caching
aurora --help              # Show help
```

## Project Structure

```
aurora/
├── crates/
│   ├── cli/       # CLI binary (aurora-cli)
│   ├── core/      # Core types and traits (aurora-core)
│   ├── parser/    # Beamfile DSL parser (aurora-parser)
│   ├── engine/    # Execution engine (aurora-engine)
│   └── plugin/    # WASM plugin system (aurora-plugin)
```

## Development

### Prerequisites

- Rust 1.85+ (edition 2024)
- Cargo

### Building

```bash
cargo build
```

### Running tests

```bash
cargo test
```

### Running with clippy

```bash
cargo clippy --all-targets --all-features
```

### Formatting

```bash
cargo fmt
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Code of Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
