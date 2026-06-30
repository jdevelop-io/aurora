# Aurora Claude Code plugin and marketplace — design

Date: 2026-06-30
Status: approved (pending spec review)

## Goal

Ship a Claude Code plugin, distributed through a marketplace hosted in this
repository, so that any project adopting Aurora can install it and let Claude
Code understand how Aurora works and how to use it: read and write `Beamfile`s,
run the `aurora` CLI, and migrate existing task runners.

Both deliverables live inside the `aurora` repository (single repo to maintain,
versioned alongside Aurora itself).

## Non-goals

- No slash commands (the CLI knowledge carried by the skill covers the same
  ground without a parallel surface to maintain).
- No MCP server (overkill; Aurora is a local CLI).
- No bundling or shipping of the `aurora` binary itself; the plugin assumes the
  user installs Aurora separately (the plugin targets projects that already use
  it).

## Repository layout

```
.claude-plugin/
  marketplace.json            # marketplace "aurora", lists the plugin
plugins/
  aurora/
    .claude-plugin/
      plugin.json             # plugin manifest
    skills/
      using-aurora/
        SKILL.md              # mental model + when to use Aurora
        references/
          beamfile-dsl.md     # full Beamfile grammar reference
          cli.md              # commands and flags reference
    agents/
      aurora-expert.md        # Beamfile authoring + migration
    hooks/
      hooks.json              # declares the two hooks
      validate-beamfile.sh    # PostToolUse validation script
      session-context.sh      # SessionStart detection script
```

Installation by an end user:

```
/plugin marketplace add jdevelop-io/aurora
/plugin install aurora
```

## Marketplace manifest

`.claude-plugin/marketplace.json` defines a marketplace named `aurora` whose
owner is `jdevelop-io`, listing a single plugin `aurora` sourced from the local
path `./plugins/aurora`. Keeping it in-repo means the marketplace is updated in
the same commit/release flow as Aurora.

## Plugin manifest

`plugins/aurora/.claude-plugin/plugin.json` declares the plugin name `aurora`,
its version, a short description, author (`jdevelop-io` / Jean-Denis VIDOT),
homepage and license (MIT). Components (skills, agents, hooks) are
auto-discovered from their conventional directories; `hooks.json` is referenced
per the plugin hooks convention.

The plugin version is decoupled from the Aurora crate version: the plugin
follows its own semantic versioning since the DSL and CLI evolve at a different
cadence than the plugin documentation.

## Component 1 — skill `using-aurora`

Progressive disclosure: a concise `SKILL.md` plus reference files loaded on
demand. This keeps Claude's context lean and makes the DSL/CLI detail easy to
maintain as Aurora evolves.

### SKILL.md

Frontmatter:

- `name: using-aurora`
- `description`: a trigger-optimised sentence covering "a `Beamfile` is present"
  and "the user mentions beams, Aurora, the task runner, or building/running
  tasks", so the skill fires reliably.

Body (concise, points to references for detail):

- **When to use**: a `Beamfile` exists at the project root, or the user asks to
  build/run/test/list tasks in an Aurora project, or asks to create/edit beams.
- **Mental model**:
  - Beams are tasks declared in a `Beamfile` (HCL-inspired DSL).
  - Dependencies form a DAG; independent beams run in parallel.
  - Caching: a beam is skipped when the SHA-256 hash of its `inputs` is
    unchanged AND every declared `output` still exists on disk.
  - Executors select where commands run: `local` (default native shell),
    `docker` (inside a container), WASM plugins for community executors.
  - The process environment is NOT inherited wholesale: only an allowlist is
    propagated. Anything a beam needs must be declared in the `environment {}`
    block or passed via `--var`.
- **Workflow**: how to read a `Beamfile`, run it (`aurora`, `aurora <beam>`,
  `--list`, `--dry-run`, `--no-cache`, `--var key=value`), and interpret output.
- **Common pitfalls**: incorrect `inputs`/`outputs` mislead the cache; forgetting
  to declare environment variables; using a beam name that is not in the DAG.
- **Pointers**: read `references/beamfile-dsl.md` before writing or heavily
  editing a `Beamfile`; read `references/cli.md` for the full flag set.

### references/beamfile-dsl.md

Full DSL reference, derived from the real `pest` grammar
(`crates/aurora-core/src/parser/aurora.pest`) so it stays exact:

- `aurora {}` block: `version`, `default`, `max_parallelism`.
- `variable "name" {}`: `default`, `description`.
- `environment {}`: `NAME = "literal"` or `NAME = shell("command")`, evaluated
  sequentially (later variables see earlier results).
- `beam "name" {}` fields: `description`, `depends_on`, `inputs`, `outputs`,
  `skip_if`, `allow_failure`, `condition { any|all = [ { shell = "..." } ] }`,
  and `run { commands = [...], executor "name" { field = "..." | var.x } }`.
- Comments (`#`), string lists, `var.x` references inside executor blocks.
- Worked examples (a small multi-beam file with dependencies, caching via
  inputs/outputs, a docker executor, a conditional beam).

### references/cli.md

Full CLI reference, derived from `crates/aurora/src/main.rs`:

- Positional `beam` argument (defaults to the `aurora { default = ... }` beam).
- `--list` / `-l`, `--dry-run`, `--no-cache`, `--var key=value` (repeatable).
- Behaviour with no beam and a TTY (picker TUI opens).
- TUI keys relevant to a run (rerun `r`, log navigation/search) at a high level.

## Component 2 — agent `aurora-expert`

A single autonomous subagent covering both authoring and migration, since they
share the same Aurora expertise.

`agents/aurora-expert.md` frontmatter: `name`, a `description` with concrete
"when to use" examples (create a Beamfile, add beams, migrate from
make/just/taskfile/npm), and a tool set sufficient to read the project, write
files, and run validation.

System prompt responsibilities:

- **Authoring**: inspect the project (languages, build/test/lint commands,
  existing scripts), propose a beam breakdown, write the `Beamfile` with correct
  `depends_on`, and set `inputs`/`outputs` so caching is sound.
- **Migration**: translate a `Makefile` / `justfile` / `Taskfile.yml` / npm
  scripts into an equivalent `Beamfile`, explicitly flagging anything that does
  not map 1:1 (e.g. shell-specific Make features, phony targets, watch tasks).
- Defers to the `using-aurora` skill for DSL/CLI detail rather than duplicating
  it.
- **Self-validation**: finishes by running `aurora --dry-run` (and `--list`) to
  confirm the file parses and the DAG resolves, reporting the result.

## Component 3 — hooks

`hooks/hooks.json` declares two hooks. Both degrade gracefully: if the `aurora`
binary is not on `PATH`, the script exits 0 without blocking or emitting noise.

### PostToolUse — Beamfile validation

- Matches `Edit` / `Write` (and `MultiEdit` if applicable) whose target path
  basename is `Beamfile`.
- Runs `validate-beamfile.sh`, which:
  - checks `aurora` is available (else exit 0);
  - runs `aurora --dry-run` from the project directory;
  - on failure, prints the parser/DAG error to stderr and exits non-zero so the
    feedback is surfaced to Claude for immediate correction;
  - on success, exits 0 silently.

### SessionStart — context injection

- Runs `session-context.sh`, which:
  - looks for a `Beamfile` at the project root (else exit 0 silently);
  - if present and `aurora` is available, emits a short note that Aurora is in
    use plus the output of `aurora --list`, so Claude knows the available beams
    from the start;
  - if `aurora` is absent, emits just the short "Aurora project detected" note.

## Language convention

Plugin content (skill, agent, hook help text) is written in **English**: it is a
community-facing surface, like the README and `docs/`. Commits remain in
**French** with gitmoji + Conventional Commits, per repository convention. No
Claude/Anthropic attribution anywhere.

## Testing and validation

- `marketplace.json` and `plugin.json` validate against the Claude Code plugin
  schema (use the plugin-validator agent / `plugin-dev` skills during build).
- Hook scripts are POSIX `sh`, executable, and tested for the graceful-degrade
  path (no `aurora` on PATH) and the happy path (valid and invalid Beamfile).
- The reference docs are cross-checked against the actual grammar and `main.rs`
  so they never drift into invented syntax.
- Manual smoke test: add the marketplace locally, install the plugin, open a
  session in a project with a `Beamfile`, confirm the SessionStart note appears
  and that editing the `Beamfile` triggers validation.

## Open questions

None blocking. Plugin versioning starts at `0.1.0`.
