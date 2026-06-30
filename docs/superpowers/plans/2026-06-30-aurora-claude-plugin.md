# Aurora Claude Code plugin and marketplace — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Claude Code plugin (skill + agent + hooks) distributed by an in-repo marketplace, so any project using Aurora can install it and have Claude Code understand and use Aurora.

**Architecture:** A marketplace manifest at the repo root (`.claude-plugin/marketplace.json`) lists a single plugin living under `plugins/aurora/`. The plugin carries a progressive-disclosure skill (`using-aurora` with two reference files), one autonomous agent (`aurora-expert` for authoring and migration), and two hooks (Beamfile validation on edit, Aurora context on session start). All hook scripts degrade gracefully when the `aurora` binary is absent.

**Tech Stack:** Claude Code plugin format (JSON manifests, Markdown skills/agents, `hooks.json` + bash scripts). Content cross-checked against the real `pest` grammar (`crates/aurora-core/src/parser/aurora.pest`) and CLI (`crates/aurora/src/main.rs`).

## Global Constraints

- Plugin/marketplace version starts at `0.1.0`, decoupled from the Aurora crate version.
- Marketplace name: `aurora`. Plugin name: `aurora`. Owner/author: `Jean-Denis VIDOT`, url `https://github.com/jdevelop-io`, email `admin@jdevelop.io`.
- Repository: `jdevelop-io/aurora`. License: MIT.
- Plugin content (skill, agent, hook text) in **English**. Git commits in **French**, gitmoji + Conventional Commits (e.g. `:sparkles: feat(plugin): ...`).
- No Claude/Anthropic attribution anywhere (commits, files, manifests).
- Hook scripts: `#!/usr/bin/env bash`, executable, graceful no-op (exit 0) when `aurora` is not on `PATH`. JSON escaping via bash parameter substitution, modelled on Claude Code's official SessionStart hook.
- Reference docs must match the real grammar/CLI — never invent syntax. Source of truth: `crates/aurora-core/src/parser/aurora.pest` and `crates/aurora/src/main.rs`.

---

## File Structure

```
.claude-plugin/marketplace.json                          # CREATE — marketplace "aurora"
plugins/aurora/.claude-plugin/plugin.json                # CREATE — plugin manifest
plugins/aurora/README.md                                 # CREATE — plugin readme
plugins/aurora/skills/using-aurora/SKILL.md              # CREATE — mental model + when to use
plugins/aurora/skills/using-aurora/references/beamfile-dsl.md  # CREATE — full DSL reference
plugins/aurora/skills/using-aurora/references/cli.md          # CREATE — full CLI reference
plugins/aurora/agents/aurora-expert.md                   # CREATE — authoring + migration agent
plugins/aurora/hooks/hooks.json                          # CREATE — declares both hooks
plugins/aurora/hooks/validate-beamfile.sh               # CREATE — PostToolUse validation
plugins/aurora/hooks/session-context.sh                 # CREATE — SessionStart context
README.md                                                # MODIFY — add a "Claude Code plugin" section
```

---

## Task 1: Marketplace and plugin manifests

**Files:**
- Create: `.claude-plugin/marketplace.json`
- Create: `plugins/aurora/.claude-plugin/plugin.json`

**Interfaces:**
- Produces: marketplace entry pointing at `./plugins/aurora`; plugin named `aurora`. Later tasks add components into `plugins/aurora/{skills,agents,hooks}/` which Claude Code auto-discovers.

- [ ] **Step 1: Write the marketplace manifest**

Create `.claude-plugin/marketplace.json`:

```json
{
  "$schema": "https://anthropic.com/claude-code/marketplace.schema.json",
  "name": "aurora",
  "version": "0.1.0",
  "description": "Marketplace for Aurora, the Rust task runner. Hosts the Aurora Claude Code plugin.",
  "owner": {
    "name": "Jean-Denis VIDOT",
    "email": "admin@jdevelop.io",
    "url": "https://github.com/jdevelop-io"
  },
  "plugins": [
    {
      "name": "aurora",
      "source": "./plugins/aurora",
      "description": "Teaches Claude Code how Aurora works: read and write Beamfiles, run the CLI, and migrate make/just/taskfile/npm scripts to Aurora.",
      "version": "0.1.0",
      "category": "development"
    }
  ]
}
```

- [ ] **Step 2: Write the plugin manifest**

Create `plugins/aurora/.claude-plugin/plugin.json`:

```json
{
  "name": "aurora",
  "version": "0.1.0",
  "description": "Teaches Claude Code how Aurora works: read and write Beamfiles, run the CLI, and migrate make/just/taskfile/npm scripts to Aurora.",
  "author": {
    "name": "Jean-Denis VIDOT",
    "url": "https://github.com/jdevelop-io"
  },
  "license": "MIT",
  "homepage": "https://github.com/jdevelop-io/aurora/tree/main/plugins/aurora",
  "keywords": [
    "aurora",
    "task-runner",
    "build-tool",
    "beamfile",
    "rust"
  ]
}
```

- [ ] **Step 3: Validate both manifests are well-formed JSON with required fields**

Run:

```bash
jq -e '.name == "aurora" and (.plugins | length) == 1 and .plugins[0].source == "./plugins/aurora"' .claude-plugin/marketplace.json
jq -e '.name == "aurora" and .version == "0.1.0" and .license == "MIT"' plugins/aurora/.claude-plugin/plugin.json
```

Expected: each command prints `true` and exits 0.

- [ ] **Step 4: Commit**

```bash
git add .claude-plugin/marketplace.json plugins/aurora/.claude-plugin/plugin.json
git commit -m ":sparkles: feat(plugin): initialiser la marketplace et le manifeste du plugin Aurora"
```

---

## Task 2: Skill `using-aurora`

**Files:**
- Create: `plugins/aurora/skills/using-aurora/SKILL.md`
- Create: `plugins/aurora/skills/using-aurora/references/beamfile-dsl.md`
- Create: `plugins/aurora/skills/using-aurora/references/cli.md`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: a skill named `using-aurora` that the agent (Task 3) and hooks (Task 4) reference by name for DSL/CLI knowledge.

- [ ] **Step 1: Write the skill entry point**

Create `plugins/aurora/skills/using-aurora/SKILL.md`:

Write the body in imperative/infinitive form (no "you"), and the description in third person with concrete trigger phrases.

```markdown
---
name: using-aurora
description: This skill should be used when a Beamfile is present in the project, or when the user mentions Aurora, beams, the task runner, or asks to "build", "run", "test", "list", "create a beam", or "edit the Beamfile" in a project that uses Aurora. Covers the Beamfile DSL, the aurora CLI, and Aurora's execution model.
---

# Using Aurora

Aurora is a task runner and build tool written in Rust, an alternative to `make`, `just`, and `taskfile`. Tasks are called **beams** and are declared in a `Beamfile` (an HCL-inspired DSL).

## When to use

- A `Beamfile` exists at the project root.
- The user asks to build, run, test, lint, or list tasks in an Aurora project.
- The user asks to create or edit beams, or to migrate another task runner to Aurora.

## Mental model

- **Beams** are named tasks declared in a `Beamfile`. Each beam runs one or more shell commands.
- **Dependencies form a DAG.** `depends_on` declares prerequisites; Aurora runs independent beams in parallel and a dependency before its dependents. Cycles are an error.
- **Caching.** A beam is skipped when the SHA-256 hash of its declared `inputs` (file contents and paths) is unchanged AND every declared `output` still exists on disk. Wrong `inputs`/`outputs` mislead the cache: too few inputs means stale results, missing outputs means needless reruns.
- **Executors** decide where commands run: `local` (the default native shell), `docker` (inside a container via the Docker CLI), and WASM plugins for community executors.
- **The process environment is NOT inherited wholesale.** Only an allowlist is propagated to beams (a Beamfile is treated as untrusted). Anything a beam needs must be declared in the `environment {}` block or passed with `--var`.

## Workflow

1. **Discover** what exists: read the `Beamfile`, or run `aurora --list` to see beams and descriptions.
2. **Run** a beam: `aurora <beam>`. With no argument, Aurora runs the `default` beam (or opens the picker TUI on a TTY).
3. **Preview** without executing: `aurora --dry-run` resolves the target and DAG and prints what would run.
4. **Override** variables at the command line: `aurora --var key=value` (repeatable).
5. **Bypass the cache** when needed: `aurora --no-cache`.

## Writing or editing a Beamfile

Read `references/beamfile-dsl.md` for the full grammar before writing or substantially editing a `Beamfile`. Set `inputs` and `outputs` deliberately so caching is correct. Declare every environment variable a command relies on.

## CLI details

Read `references/cli.md` for the complete flag set and behaviours.

## Common pitfalls

- Declaring too few `inputs` (stale cache hits) or forgetting `outputs` (cache never validates).
- Relying on an environment variable without declaring it in `environment {}` or passing `--var`.
- Referencing a beam name in `depends_on` that does not exist (DAG error) or that forms a cycle.

## Additional resources

- **`references/beamfile-dsl.md`** — the complete Beamfile grammar (blocks, beam fields, conditions, executors), with worked examples. Read it before writing or substantially editing a `Beamfile`.
- **`references/cli.md`** — every CLI flag and behaviour, with examples. Read it for the full command surface.
```

- [ ] **Step 2: Write the Beamfile DSL reference**

Create `plugins/aurora/skills/using-aurora/references/beamfile-dsl.md`. This content mirrors the `pest` grammar at `crates/aurora-core/src/parser/aurora.pest`:

````markdown
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
  skip_if       = "test -f .skip-tests"      # shell command; non-zero exit means do not skip
  allow_failure = false              # when true, a failure counts as success for scheduling

  run {
    commands = ["cargo test --workspace"]
  }
}
```

### `condition` block

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
````

- [ ] **Step 3: Write the CLI reference**

Create `plugins/aurora/skills/using-aurora/references/cli.md`. This mirrors the clap setup in `crates/aurora/src/main.rs`:

```markdown
# Aurora CLI reference

```
aurora [BEAM] [FLAGS]
```

Aurora reads the `Beamfile` in the current directory.

## Positional argument

- `BEAM` — the beam to run. When omitted, Aurora runs the `default` beam from the `aurora {}` block. With no default and a TTY, the picker TUI opens (fuzzy search; multi-select runs the selected beams via a virtual aggregate beam).

## Flags

- `-l`, `--list` — print every beam with its description and exit. Output:

  ```
  Available beams:
    fmt                   Format code
    test                  Run tests
  ```

- `--dry-run` — resolve the target and DAG, print `Would execute beam: <name>`, and exit without running anything.
- `--no-cache` — ignore the cache; run every beam even when inputs are unchanged.
- `--var key=value` — override a variable's default. Repeatable: `--var a=1 --var b=2`. Invalid format (missing `=`) is an error.

## TUI keys (during a run)

- `r` — rerun the focused beam, reusing already-succeeded beams instead of re-running them.
- Log navigation and search are available in the execution view.

## Examples

```bash
aurora                      # run the default beam
aurora test                 # run the "test" beam and its dependencies
aurora --list               # list beams
aurora --dry-run test       # show what "test" would run
aurora --no-cache build     # force a rebuild
aurora --var profile=release build
```
```

- [ ] **Step 4: Verify the skill frontmatter and references resolve**

Run:

```bash
test -f plugins/aurora/skills/using-aurora/SKILL.md
head -1 plugins/aurora/skills/using-aurora/SKILL.md | grep -qx -- '---'
grep -q '^name: using-aurora$' plugins/aurora/skills/using-aurora/SKILL.md
grep -q '^description: This skill should be used when' plugins/aurora/skills/using-aurora/SKILL.md
test -f plugins/aurora/skills/using-aurora/references/beamfile-dsl.md
test -f plugins/aurora/skills/using-aurora/references/cli.md
echo OK
```

Expected: prints `OK` (all checks pass, exit 0).

- [ ] **Step 5: Cross-check the DSL reference against the real grammar**

Run:

```bash
grep -oE 'beam_(description|depends_on|inputs|outputs|skip_if|allow_failure|condition|run)' crates/aurora-core/src/parser/aurora.pest | sort -u
```

Expected: lists `beam_allow_failure`, `beam_condition`, `beam_depends_on`, `beam_description`, `beam_inputs`, `beam_outputs`, `beam_run`, `beam_skip_if`. Confirm every one of these fields appears in `references/beamfile-dsl.md` (description, depends_on, inputs, outputs, skip_if, allow_failure, condition, run). Fix the reference if any field is missing or misspelled.

- [ ] **Step 6: Commit**

```bash
git add plugins/aurora/skills/
git commit -m ":sparkles: feat(plugin): ajouter la skill using-aurora et ses références"
```

---

## Task 3: Agent `aurora-expert`

**Files:**
- Create: `plugins/aurora/agents/aurora-expert.md`

**Interfaces:**
- Consumes: the `using-aurora` skill (Task 2) for DSL/CLI knowledge.
- Produces: an agent named `aurora-expert` discoverable by Claude Code.

- [ ] **Step 1: Write the agent definition**

Create `plugins/aurora/agents/aurora-expert.md`:

```markdown
---
name: aurora-expert
description: Use this agent to author or migrate Aurora Beamfiles. Typical triggers include "create a Beamfile", "add beams", "set up Aurora for this project", and "migrate my Makefile/justfile/Taskfile/npm scripts to Aurora". See "When to invoke" in the agent body for worked scenarios.
model: inherit
color: magenta
tools: ["Read", "Glob", "Grep", "Write", "Edit", "Bash"]
---

You are an Aurora expert. You author and migrate `Beamfile`s for projects that use Aurora, the Rust task runner. Follow the `using-aurora` skill for the Beamfile DSL and CLI details; do not invent syntax.

## When to invoke

- **Author a Beamfile from scratch.** The project has no `Beamfile` and the user wants to set up Aurora. Inspect the project and design the beams.
- **Extend an existing Beamfile.** The user wants to add or restructure beams, or fix a broken DAG.
- **Migrate another task runner.** The user wants to convert a `Makefile`, `justfile`, `Taskfile.yml`, or npm scripts into an equivalent `Beamfile`.

## Operating modes

### Authoring (create or extend a Beamfile)

1. Inspect the project: detect languages, package manifests, and existing build/test/lint commands (`package.json` scripts, `Cargo.toml`, `Makefile`, CI config).
2. Propose a beam breakdown: one beam per meaningful task (format, lint, test, build, etc.), with `depends_on` capturing real prerequisites so the DAG enables parallelism.
3. Set `inputs` and `outputs` deliberately so caching is sound: inputs are the files a beam reads; outputs are the artifacts it produces and that must exist for a cache hit.
4. Declare every environment variable a command relies on in the `environment {}` block (the process environment is not inherited wholesale).

### Migration (convert another task runner)

1. Read the source file (`Makefile`, `justfile`, `Taskfile.yml`, or `package.json` scripts).
2. Translate each target/task into an equivalent beam, mapping dependencies to `depends_on`.
3. Explicitly flag what does not map 1:1, for example: phony targets, pattern rules, shell-specific Make features, file-watching/`watch` tasks, and interactive tasks. Note these in your summary rather than silently dropping them.

## Always finish by validating

Run `aurora --dry-run` (and `aurora --list`) from the project directory to confirm the Beamfile parses and the DAG resolves. If `aurora` is not installed, say so and ask the user to install it or validate manually. Report the validation result and a short summary of the beams you created or migrated, including anything that did not translate cleanly.
```

- [ ] **Step 2: Verify the agent frontmatter**

Run:

```bash
grep -q '^name: aurora-expert$' plugins/aurora/agents/aurora-expert.md
grep -q '^description: Use this agent' plugins/aurora/agents/aurora-expert.md
grep -q '^model: inherit$' plugins/aurora/agents/aurora-expert.md
grep -q '^color: ' plugins/aurora/agents/aurora-expert.md
grep -q '^tools: ' plugins/aurora/agents/aurora-expert.md
grep -q '## When to invoke' plugins/aurora/agents/aurora-expert.md
echo OK
```

Expected: prints `OK`.

- [ ] **Step 3: Commit**

```bash
git add plugins/aurora/agents/aurora-expert.md
git commit -m ":sparkles: feat(plugin): ajouter l'agent aurora-expert (authoring et migration)"
```

---

## Task 4: Hooks (validation + session context)

**Files:**
- Create: `plugins/aurora/hooks/hooks.json`
- Create: `plugins/aurora/hooks/validate-beamfile.sh`
- Create: `plugins/aurora/hooks/session-context.sh`

**Interfaces:**
- Consumes: nothing from earlier tasks (hooks are self-contained scripts).
- Produces: two registered hooks. Both exit 0 silently when `aurora` is not on `PATH`.

**Hook I/O contract:**
- Each hook receives the event as JSON on stdin. PostToolUse JSON contains `.tool_input.file_path` (the edited file) and `.cwd`. SessionStart JSON contains `.cwd`.
- PostToolUse: to surface a validation error to Claude, exit non-zero with the message on stderr; exit 0 to stay silent.
- SessionStart: print `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"..."}}` on stdout to inject context.

- [ ] **Step 1: Write the validation script**

Create `plugins/aurora/hooks/validate-beamfile.sh`:

```bash
#!/usr/bin/env bash
# PostToolUse hook: validate a Beamfile right after it is edited.
# Reads the hook event JSON on stdin, extracts the edited file path, and if it
# is a Beamfile runs `aurora --dry-run` to parse it and resolve the DAG.
# Degrades gracefully: if aurora is not installed, exits 0 without blocking.

set -euo pipefail

input="$(cat)"

# Extract the edited file path from the tool input JSON.
file_path="$(printf '%s' "$input" \
  | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
  | head -n1)"

# Only act on files literally named Beamfile.
case "$(basename "$file_path" 2>/dev/null)" in
  Beamfile) ;;
  *) exit 0 ;;
esac

# No binary -> nothing to validate, stay silent.
command -v aurora >/dev/null 2>&1 || exit 0

dir="$(dirname "$file_path")"
if ! output="$(cd "$dir" && aurora --dry-run 2>&1)"; then
  printf 'Beamfile validation failed (aurora --dry-run):\n%s\n' "$output" >&2
  exit 2
fi

exit 0
```

- [ ] **Step 2: Write the session-context script**

Create `plugins/aurora/hooks/session-context.sh`:

```bash
#!/usr/bin/env bash
# SessionStart hook: when the project uses Aurora (a Beamfile is present),
# inject a short note plus the available beams so Claude knows Aurora is here.
# Degrades gracefully: emits a note without the beam list if aurora is absent;
# stays silent (no output) when there is no Beamfile.

set -euo pipefail

input="$(cat)"

# Prefer the project root Claude Code exposes; fall back to the event cwd, then pwd.
cwd="${CLAUDE_PROJECT_DIR:-}"
if [ -z "$cwd" ]; then
  cwd="$(printf '%s' "$input" \
    | sed -n 's/.*"cwd"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n1)"
fi
[ -n "$cwd" ] || cwd="$(pwd)"

# No Beamfile -> not an Aurora project, stay silent.
[ -f "$cwd/Beamfile" ] || exit 0

if command -v aurora >/dev/null 2>&1; then
  beams="$(cd "$cwd" && aurora --list 2>/dev/null || true)"
  context="This project uses Aurora (a Beamfile is present). Use the using-aurora skill to read or edit it and to run the CLI."$'\n\n'"$beams"
else
  context="This project uses Aurora (a Beamfile is present), but the 'aurora' binary is not installed. Use the using-aurora skill for the DSL; ask the user to install Aurora to run beams."
fi

# Escape the context for safe JSON embedding (bash parameter substitution).
escape_for_json() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

escaped="$(escape_for_json "$context")"
printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$escaped"

exit 0
```

- [ ] **Step 3: Write the hooks manifest and make scripts executable**

Create `plugins/aurora/hooks/hooks.json`:

```json
{
  "description": "Aurora plugin hooks: validate Beamfiles on edit, inject Aurora context on session start.",
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "bash \"${CLAUDE_PLUGIN_ROOT}/hooks/validate-beamfile.sh\"",
            "timeout": 30
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "matcher": "startup|resume|clear",
        "hooks": [
          {
            "type": "command",
            "command": "bash \"${CLAUDE_PLUGIN_ROOT}/hooks/session-context.sh\"",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

Then:

```bash
chmod +x plugins/aurora/hooks/validate-beamfile.sh plugins/aurora/hooks/session-context.sh
```

- [ ] **Step 4: Test — validation script ignores non-Beamfile edits**

Run:

```bash
printf '{"tool_name":"Edit","tool_input":{"file_path":"/tmp/notes.txt"}}' \
  | bash plugins/aurora/hooks/validate-beamfile.sh; echo "exit=$?"
```

Expected: no output, `exit=0`.

- [ ] **Step 5: Test — validation script degrades gracefully without aurora on PATH**

Run with a minimal PATH that keeps the standard utilities (`cat`, `sed`, `basename`, `dirname`) but excludes any user-installed `aurora` (which lives in `~/.cargo/bin`, `~/.local/bin`, or `/usr/local/bin`):

```bash
printf '{"tool_name":"Edit","tool_input":{"file_path":"/tmp/Beamfile"}}' \
  | PATH="/usr/bin:/bin" bash plugins/aurora/hooks/validate-beamfile.sh; echo "exit=$?"
```

Expected: no output, `exit=0` (the `command -v aurora` check fails, so the script exits silently).

- [ ] **Step 6: Test — session-context stays silent outside an Aurora project**

Run:

```bash
tmp="$(mktemp -d)"
printf '{"cwd":"%s"}' "$tmp" \
  | CLAUDE_PROJECT_DIR="$tmp" bash plugins/aurora/hooks/session-context.sh; echo "exit=$?"
rm -rf "$tmp"
```

Expected: no output, `exit=0` (no Beamfile in the directory).

- [ ] **Step 7: Test — session-context emits context for an Aurora project (no binary)**

Run:

```bash
tmp="$(mktemp -d)"; : > "$tmp/Beamfile"
out="$(printf '{"cwd":"%s"}' "$tmp" | CLAUDE_PROJECT_DIR="$tmp" PATH="/usr/bin:/bin" bash plugins/aurora/hooks/session-context.sh)"
rm -rf "$tmp"
printf '%s\n' "$out" | jq -e '.hookSpecificOutput.hookEventName == "SessionStart" and (.hookSpecificOutput.additionalContext | contains("Aurora"))'
```

Expected: prints `true` (valid JSON, mentions Aurora). The `binary is not installed` branch is taken.

- [ ] **Step 8: Commit**

```bash
git add plugins/aurora/hooks/
git commit -m ":sparkles: feat(plugin): ajouter les hooks de validation Beamfile et de contexte de session"
```

---

## Task 5: Plugin README, repository README section, and final validation

**Files:**
- Create: `plugins/aurora/README.md`
- Modify: `README.md` (add a "Claude Code plugin" section near the end, before any license/footer section)

**Interfaces:**
- Consumes: the manifests, skill, agent, and hooks from Tasks 1-4.
- Produces: user-facing install/usage docs; no code other tasks depend on.

- [ ] **Step 1: Write the plugin README**

Create `plugins/aurora/README.md`:

````markdown
# Aurora Claude Code plugin

Teaches [Claude Code](https://claude.ai/code) how Aurora works so it can read and write `Beamfile`s, run the `aurora` CLI, and migrate other task runners.

## What it provides

- **Skill `using-aurora`** — Aurora's execution model, the Beamfile DSL, and the CLI, with reference files loaded on demand.
- **Agent `aurora-expert`** — authors `Beamfile`s and migrates `make`/`just`/`taskfile`/npm scripts to Aurora.
- **Hooks** — validates a `Beamfile` after it is edited (`aurora --dry-run`) and announces available beams at session start. Both no-op silently when the `aurora` binary is not installed.

## Install

```text
/plugin marketplace add jdevelop-io/aurora
/plugin install aurora
```

The plugin assumes the `aurora` binary is installed separately (see the repository README for installation).

## License

MIT.
````

- [ ] **Step 2: Add a section to the repository README**

In `README.md`, add the following section. Place it after the main usage/features content and before the license/footer (use `grep -n '^## ' README.md` to find a suitable insertion point such as just before a `## License`/`## Contributing` section, or append at the end if none exists):

```markdown
## Claude Code plugin

Aurora ships a [Claude Code](https://claude.ai/code) plugin so the assistant understands Aurora and can read, write, and run `Beamfile`s in your project. Install it from this repository's marketplace:

```text
/plugin marketplace add jdevelop-io/aurora
/plugin install aurora
```

It adds a skill (Aurora's model, the Beamfile DSL, and the CLI), an `aurora-expert` agent (authoring and migration from make/just/taskfile/npm), and hooks that validate Beamfiles on edit and surface available beams at session start. See [`plugins/aurora`](plugins/aurora) for details.
```

- [ ] **Step 3: Verify the README section and links**

Run:

```bash
grep -q '## Claude Code plugin' README.md
test -f plugins/aurora/README.md
grep -q '/plugin marketplace add jdevelop-io/aurora' README.md
echo OK
```

Expected: prints `OK`.

- [ ] **Step 4: Final structural validation of the whole plugin**

Run:

```bash
jq empty .claude-plugin/marketplace.json
jq empty plugins/aurora/.claude-plugin/plugin.json
jq empty plugins/aurora/hooks/hooks.json
for f in \
  plugins/aurora/skills/using-aurora/SKILL.md \
  plugins/aurora/skills/using-aurora/references/beamfile-dsl.md \
  plugins/aurora/skills/using-aurora/references/cli.md \
  plugins/aurora/agents/aurora-expert.md \
  plugins/aurora/hooks/validate-beamfile.sh \
  plugins/aurora/hooks/session-context.sh \
  plugins/aurora/README.md ; do
  test -f "$f" || { echo "MISSING: $f"; exit 1; }
done
test -x plugins/aurora/hooks/validate-beamfile.sh
test -x plugins/aurora/hooks/session-context.sh
echo "ALL PRESENT"
```

Expected: prints `ALL PRESENT` with no `MISSING` lines and exit 0.

- [ ] **Step 5: Optional — run the plugin-validator agent**

If available, dispatch the `plugin-dev:plugin-validator` agent against `plugins/aurora` to catch schema issues the structural checks miss. Address any reported errors, then re-run Step 4.

- [ ] **Step 6: Manual smoke test (requires a built aurora binary)**

This step is manual and needs the `aurora` binary on `PATH` (`cargo build --release` then add `target/release` to `PATH`, or install via the repo scripts). In a checkout of this repo (which has a `Beamfile`):

```text
/plugin marketplace add /Users/jh3ady/Personal/OpenSource/aurora
/plugin install aurora
```

Then open a fresh Claude Code session in this repo and confirm: the SessionStart context note lists the repo's beams (`fmt`, `clippy`, `test`, `build`, `check`); editing the `Beamfile` to introduce a syntax error triggers the validation hook and surfaces the parser error. Revert the test edit afterward.

- [ ] **Step 7: Commit**

```bash
git add plugins/aurora/README.md README.md
git commit -m ":memo: docs(plugin): documenter l'installation et l'usage du plugin Aurora"
```

---

## Self-Review notes

- **Spec coverage:** marketplace (Task 1), plugin manifest (Task 1), skill with progressive disclosure + two references (Task 2), agent authoring+migration (Task 3), both hooks with graceful degradation (Task 4), English content / French commits / no attribution (Global Constraints + commit messages), testing and validation (Tasks 4-5). All spec sections map to a task.
- **No placeholders:** every file's full content is inline; every test has a concrete command and expected output.
- **Naming consistency:** `using-aurora`, `aurora-expert`, `validate-beamfile.sh`, `session-context.sh`, `hooks.json`, marketplace/plugin name `aurora` are used identically across tasks, manifests, README, and verification commands.
- **Known limitation:** hook JSON parsing uses `sed` (no `jq`/`python` dependency assumed). It extracts the first `file_path`/`cwd` string value, which is sufficient for the single-file Edit/Write events and the SessionStart `cwd` field. Documented in the validation contract.
````
