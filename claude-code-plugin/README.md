# Aurora Claude Code plugin

Teaches [Claude Code](https://claude.ai/code) how Aurora works so it can read and write `Beamfile`s, run the `aurora` CLI, and migrate other task runners.

## What it provides

- **Skill `using-aurora`**: Aurora's execution model, the Beamfile DSL, and the CLI, with reference files loaded on demand.
- **Agent `aurora-expert`**: authors `Beamfile`s and migrates `make`/`just`/`taskfile`/npm scripts to Aurora.
- **Hooks**: validates a `Beamfile` after it is edited (`aurora --dry-run`) and announces available beams at session start. Both no-op silently when the `aurora` binary is not installed.

## Install

```text
/plugin marketplace add jdevelop-io/aurora
/plugin install aurora
```

The plugin assumes the `aurora` binary is installed separately (see the repository README for installation).

## License

MIT.
