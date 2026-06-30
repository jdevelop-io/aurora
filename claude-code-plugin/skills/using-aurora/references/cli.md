# Aurora CLI reference

```
aurora [BEAM] [FLAGS]
```

Aurora reads the `Beamfile` in the current directory.

## Positional argument

- `BEAM` — the beam to run. When omitted on a TTY, the picker TUI opens (fuzzy search; multi-select runs the selected
  beams via a virtual aggregate beam). The `default` beam from the `aurora {}` block is currently consulted only by
  `--dry-run` to choose the target it prints, not to run a beam directly.

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
