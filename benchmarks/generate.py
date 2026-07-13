#!/usr/bin/env python3
"""Emit the same task graph as a Beamfile, a Makefile, a justfile and a Taskfile.

One graph definition, four files. A benchmark that hand-writes each runner's
input separately is worthless: any difference it measures could just be a
difference in how generously each file was written.
"""

import argparse
import pathlib
import sys


def fan_out(n: int, body: str) -> tuple[list[str], dict[str, list[str]], dict[str, str]]:
    """`n` independent tasks, plus an aggregate that depends on all of them.

    The shape that separates a runner which parallelises from one which does not.
    """
    tasks = [f"t{i}" for i in range(1, n + 1)]
    deps = {"all": tasks}
    cmds = {t: body for t in tasks}
    return tasks + ["all"], deps, cmds


def chain(n: int, body: str) -> tuple[list[str], dict[str, list[str]], dict[str, str]]:
    """A linear dependency chain: t1 -> t2 -> ... -> tn.

    No runner can parallelise this, so it isolates per-task scheduling overhead
    from any parallelism gain.
    """
    tasks = [f"t{i}" for i in range(1, n + 1)]
    deps = {tasks[i]: [tasks[i - 1]] for i in range(1, n)}
    deps["all"] = [tasks[-1]]
    cmds = {t: body for t in tasks}
    return tasks + ["all"], deps, cmds


SHAPES = {"fan-out": fan_out, "chain": chain}


def write_beamfile(path, order, deps, cmds):
    lines = ['aurora {', '  version = "1"', '  default = "all"', "}", ""]
    for name in order:
        lines.append(f'beam "{name}" {{')
        if deps.get(name):
            joined = ", ".join(f'"{d}"' for d in deps[name])
            lines.append(f"  depends_on = [{joined}]")
        if name in cmds:
            lines.append(f'  run {{ commands = ["{cmds[name]}"] }}')
        lines.append("}")
        lines.append("")
    path.write_text("\n".join(lines))


def write_makefile(path, order, deps, cmds):
    lines = [f".PHONY: {' '.join(order)}", ""]
    for name in order:
        prereqs = " ".join(deps.get(name, []))
        lines.append(f"{name}: {prereqs}".rstrip())
        lines.append(f"\t@{cmds.get(name, 'true')}")
        lines.append("")
    path.write_text("\n".join(lines))


def write_justfile(path, order, deps, cmds):
    lines = []
    for name in order:
        prereqs = " ".join(deps.get(name, []))
        lines.append(f"{name}: {prereqs}".rstrip())
        lines.append(f"    @{cmds.get(name, 'true')}")
        lines.append("")
    path.write_text("\n".join(lines))


def write_taskfile(path, order, deps, cmds):
    lines = ["version: '3'", "", "tasks:"]
    for name in order:
        lines.append(f"  {name}:")
        if deps.get(name):
            # `deps` is what Task runs concurrently; `cmds` would serialise them.
            lines.append(f"    deps: [{', '.join(deps[name])}]")
        lines.append(f"    cmds: ['{cmds.get(name, 'true')}']")
    path.write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shape", choices=sorted(SHAPES), default="fan-out")
    parser.add_argument("--tasks", type=int, default=8)
    parser.add_argument("--body", default="true", help="the command every task runs")
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("."))
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    order, deps, cmds = SHAPES[args.shape](args.tasks, args.body)

    write_beamfile(args.out / "Beamfile", order, deps, cmds)
    write_makefile(args.out / "Makefile", order, deps, cmds)
    write_justfile(args.out / "justfile", order, deps, cmds)
    write_taskfile(args.out / "Taskfile.yml", order, deps, cmds)

    print(f"{args.shape}: {args.tasks} tasks, body={args.body!r} -> {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
