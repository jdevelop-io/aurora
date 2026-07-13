# Benchmarks

Aurora positions itself against `make`, `just` and `taskfile`. This directory
exists so that claim can be checked rather than believed, including when the
answer is unflattering. It currently is.

## Running them

```bash
cargo build --release
./benchmarks/run.sh                 # performance -> benchmarks/results.md
./benchmarks/cache-correctness.sh   # what each runner actually re-runs
```

Needs `hyperfine`, `just`, `task`, `python3` and GNU `make` 4.x (`brew install
hyperfine just go-task make`; macOS ships GNU make 3.81 from 2006, and
benchmarking against that would be a rigged fight, so the script prefers a
modern one when present).

`generate.py` emits the same task graph as a `Beamfile`, a `Makefile`, a
`justfile` and a `Taskfile.yml`. One definition, four files: a benchmark that
hand-writes each runner's input separately measures how generously each file was
written, not the runners.

The numbers in [`results.md`](results.md) were measured on one machine and are
reproduced there with its specs. Rerun them on yours rather than trusting them.

### The one trap worth knowing about

The no-op body is an **absolute path** to a real binary, not the bare word
`true`. Task embeds a Go shell interpreter (`mvdan/sh`) which runs `true`, `echo`
and `cd` as in-process builtins: given a bare `true`, Task forks nothing at all,
while the other three fork and exec a hundred processes. Comparing those two
things and calling it a benchmark is exactly the kind of unexamined premise this
directory was built to catch. The "Shell builtins" scenario keeps that case
around explicitly, because it is a real advantage of Task's design.

## What the numbers say

Apple M5, 10 cores, 30 runs per case. Aurora 0.8.0.

**Aurora parallelises by default, and so does Task.** On 8 independent tasks of
0.5s: `make -j10` 524ms, Aurora 538ms, Task 547ms. `just` takes 4.1s, and so does
`make` without `-j`. The runner that genuinely cannot run dependencies in
parallel is `just`; `make` can, but only if the user remembers the flag; Task
runs its `deps` concurrently out of the box, exactly like Aurora. "No mainstream
task runner offers this" was not true.

**Aurora starts processes about as fast as `make`.** On 100 independent tasks that
exec a real binary: `make -j10` 19.6ms, Aurora 20.7ms, Task 130ms, `just` 357ms.
On a 50-task chain, where nothing can be parallelised: make 42ms, Aurora 46ms,
Task 108ms.

It was not always so. Aurora used to be 4x `make` here (75.5ms), for two reasons
that had nothing to do with scheduling. It always spawned `sh -c`, where `make`
execs the command directly when it needs no shell. And it passed a bare program
name, which makes Rust's standard library fall back from `posix_spawn` to `fork` +
`exec` — expensive for a 24 MB binary that links wasmtime, where `make` is 244 KB.
Fixing both is what closed the gap; the scheduler was never the problem.

**Aurora beats `make` at equal features.** `make -j` interleaves the output of
parallel jobs into unreadable soup unless you ask for `--output-sync`, which
captures it. Aurora always captures, so that is the honest comparison: Aurora
21.0ms, `make -j10 --output-sync` 25.1ms.

**Where Task wins, and where it does not.** Task embeds a Go shell interpreter
(`mvdan/sh`) that runs builtins in-process without forking. That is a real design
advantage on shell-only tasks. But any real command — a compiler, a linter, a
container — has to be exec'd, and there Task is 6x slower than Aurora and `make`.

**The cache is the differentiator, and it is a correctness argument, not a speed
one.** Change a task's *command* while leaving its input files untouched:

| runner   | caches an unchanged re-run | after the command changes |
|----------|---------------------------|---------------------------|
| `aurora` | yes                       | **re-runs**               |
| `make`   | yes                       | **serves a stale result** |
| `task`   | yes                       | **serves a stale result** |
| `just`   | no cache at all           | re-runs (it always does)  |

`make` compares timestamps of prerequisites, and Task checksums the files listed
in `sources`. Neither looks at the recipe. Edit the command and both hand you back
an artefact their current definition would never produce. Aurora hashes the beam's
inputs *and* its definition (the resolved commands, the executor and its
configuration, the `dir`, the declared environment), so the question its cache
answers is "would running this beam produce the same result?" rather than merely
"did its input files change?".

That property, combined with parallel-by-default execution and a live TUI, is
what Aurora actually has. Not throughput.
