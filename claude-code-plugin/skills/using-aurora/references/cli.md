# Aurora CLI reference

```
aurora [BEAM] [FLAGS]
```

Aurora reads the `Beamfile` in the current directory.

## Positional argument

- `BEAM` — the beam to run. When omitted on a TTY (and without `--no-tui`), the picker TUI opens (fuzzy search;
  multi-select runs the selected beams via a virtual aggregate beam). In headless mode (no TTY, or `--no-tui`),
  the `default` beam from the `aurora {}` block IS used to run when no beam is given, since there is no picker.
  `-i`/`--interactive` forces the TUI even without a terminal, so the absence of a TTY does not always mean headless.

## Flags

- `-l`, `--list` — print every beam with its description and exit. Output:

  ```
  Available beams:
    fmt                   Format code
    test                  Run tests
  ```

- `--dry-run` — resolve the target beam name (honouring `default` when no beam is given) and print
  `Would execute beam: <name>`, then exit without running anything. It prints only the target name, not the full DAG.
- `--no-cache` — currently has no effect: the flag is accepted but not yet wired, so caching is always applied. To
  force a beam to re-run, change one of its declared `inputs` or delete its entry under `.aurora/cache/`.
- `--var key=value` — override a variable's default. Repeatable: `--var a=1 --var b=2`. Invalid format (missing `=`) is an error.
- `--no-tui` — force plain, non-interactive output even in a terminal. Output is streamed per beam (lines
  prefixed with the beam name, stdout and stderr kept separate) and ends with an ASCII recap
  (`[OK]`/`[FAIL]`/`[SKIP]`/`[WARN]`/`[CANC]`) plus a `Done: N ok, M failed` summary.
- `-i`, `--interactive` — force the TUI even when output is not a terminal. Mutually exclusive with `--no-tui`.

## Output mode and exit codes

Aurora auto-detects the output mode via `stdout().is_terminal()`: a TTY gets the TUI, a pipe/redirect gets
headless. `--no-tui` and `-i` override this. ANSI colour is applied to a given stream (stdout or stderr) only
when that stream itself is a terminal and `NO_COLOR` is unset, so redirecting one stream does not leak colour
codes into it. Headless exit codes: `0` if all beams succeed (`allow_failure` beams count as success), `1` if
any beam fails, which also covers a Beamfile error detected at DAG construction (a dependency cycle or an
unknown dependency). This makes Aurora usable as a CI step:

```bash
aurora test --no-tui   # plain logs, exit 1 on failure
aurora build | tee build.log   # auto-headless because stdout is piped
```

## TUI keys (during a run)

- `r` — rerun the focused beam, reusing already-succeeded beams instead of re-running them.
- Log navigation and search are available in the execution view.

## Examples

```bash
aurora                      # open the picker (no beam given, on a TTY)
aurora test                 # run the "test" beam and its dependencies
aurora --list               # list beams
aurora --dry-run test       # show which beam "test" resolves to
aurora --var profile=release build
```
