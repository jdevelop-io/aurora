# Contributing to Aurora

Thank you for your interest in contributing to Aurora! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites

- Rust 1.85+ (edition 2024)
- Git

### Setting up the development environment

1. Fork and clone the repository:
   ```bash
   git clone https://github.com/YOUR_USERNAME/aurora.git
   cd aurora
   ```

2. Build the project:
   ```bash
   cargo build
   ```

3. Run tests:
   ```bash
   cargo test
   ```

4. Run the linter:
   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

## Development Workflow

### Branching Strategy

- `main` - stable, release-ready code
- Feature branches - `feature/description`
- Bug fix branches - `fix/description`

### Making Changes

1. Create a new branch from `main`:
   ```bash
   git checkout -b feature/my-feature
   ```

2. Make your changes following the code style guidelines

3. Add tests for new functionality

4. Ensure all tests pass:
   ```bash
   cargo test
   ```

5. Ensure code is properly formatted:
   ```bash
   cargo fmt
   ```

6. Ensure clippy passes:
   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

7. Commit your changes with a clear message:
   ```bash
   git commit -m "feat: add new feature description"
   ```

### Commit Message Convention

We follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` - new feature
- `fix:` - bug fix
- `docs:` - documentation changes
- `style:` - formatting, missing semicolons, etc.
- `refactor:` - code refactoring
- `test:` - adding or updating tests
- `chore:` - maintenance tasks

Examples:
```
feat: add variable interpolation support
fix: handle empty Beamfile gracefully
docs: update README with new CLI options
refactor: simplify DAG cycle detection
test: add integration tests for cache
```

## Code Style

### Rust Guidelines

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` with default settings
- Keep functions focused and small
- Prefer explicit error handling over panics
- Document public APIs with doc comments

### Error Handling

- Use `thiserror` for defining error types
- Use `miette` for user-facing errors with source spans
- Provide helpful error messages

### Testing

- Write unit tests in `#[cfg(test)]` modules
- Test edge cases and error conditions
- Use descriptive test names

Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_beamfile() {
        let result = parse_beamfile("");
        assert!(result.is_ok());
    }

    #[test]
    fn test_cycle_detection() {
        // ...
    }
}
```

## Project Structure

```
aurora/
├── crates/
│   ├── cli/       # CLI binary
│   ├── core/      # Core types
│   ├── parser/    # DSL parser
│   ├── engine/    # Execution engine
│   └── plugin/    # WASM plugins
├── Cargo.toml     # Workspace config
├── README.md
├── CONTRIBUTING.md
└── ...
```

### Adding a New Crate

If you need to add a new crate:

1. Create the crate directory under `crates/`
2. Add it to the workspace in the root `Cargo.toml`
3. Follow the naming convention: directory `name/`, package `aurora-name`

## Pull Request Process

1. Ensure your PR has a clear title and description
2. Link any related issues
3. Ensure CI passes (tests, clippy, fmt)
4. Request review from maintainers
5. Address review feedback
6. Squash commits if requested

### PR Title Format

Follow the commit message convention:
```
feat: add support for conditional beams
fix: resolve cache invalidation issue
```

## Reporting Issues

### Bug Reports

Include:
- Aurora version (`aurora --version`)
- Operating system
- Steps to reproduce
- Expected vs actual behavior
- Beamfile content (if relevant)

### Feature Requests

Include:
- Clear description of the feature
- Use cases
- Proposed syntax/API (if applicable)

## Questions?

- Open a GitHub Discussion for questions
- Check existing issues before creating new ones

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
