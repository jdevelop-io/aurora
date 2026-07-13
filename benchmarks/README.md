# Benchmarks

Aurora positions itself against `make`, `just` and `taskfile`. This directory
exists so that claim can be checked rather than believed, including when the
answer is unflattering.

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

## What the numbers say

Measured on an Apple M5 (10 cores), 10 runs per case.

**Aurora parallelises by default, and so does Task.** On 8 independent tasks of
0.5s: Aurora 538ms, `make -j10` 524ms, Task 552ms. `just` takes 4.1s, and so does
`make` without `-j`. The runner that genuinely cannot run dependencies in
parallel is `just`; `make` can, but only if the user remembers the flag; Task
runs its `deps` concurrently out of the box, exactly like Aurora.

**Aurora is not the fastest, and it is not close.** On 100 tasks that do nothing,
which measures nothing but the runner itself, `make -j` takes 20ms, Task 40ms and
Aurora 64ms. On a 50-task chain, Aurora spends ~2.6ms per task against ~0.7ms for
Task. Aurora carries the highest scheduling overhead of the three runners that
parallelise. On a real graph, where tasks cost hundreds of milliseconds each, tens
of milliseconds of scheduling disappear into the noise. It is still the honest
result, and speed is not an argument Aurora gets to make.

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
