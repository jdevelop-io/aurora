#!/usr/bin/env bash
#
# Does each runner notice that a task's *command* changed, when its input files
# did not? A cache that answers "no" hands back a result the current definition
# would never produce.
#
# Usage: benchmarks/cache-correctness.sh
#
# This is a correctness probe, not a benchmark: it reports what ran, not how fast.

set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if command -v gmake >/dev/null 2>&1; then
  MAKE=gmake
elif [ -x /opt/homebrew/opt/make/libexec/gnubin/make ]; then
  MAKE=/opt/homebrew/opt/make/libexec/gnubin/make
else
  MAKE="make"
fi

AURORA="${AURORA:-$root/target/release/aurora}"
[ -x "$AURORA" ] || { echo "aurora binary not found at $AURORA (cargo build --release)" >&2; exit 1; }

# Every execution appends a line to runs.log, so counting its lines tells us
# exactly what ran. `in.txt` is never touched: only the command changes.
write_files() {
  local dir="$1" cmd="$2"

  cat > "$dir/Beamfile" <<EOF
beam "build" {
  inputs  = ["in.txt"]
  outputs = ["out.txt"]
  run { commands = ["$cmd"] }
}
EOF

  printf 'out.txt: in.txt\n\t@%s\n' "$cmd" > "$dir/Makefile"
  printf 'build:\n    @%s\n' "$cmd" > "$dir/justfile"

  cat > "$dir/Taskfile.yml" <<EOF
version: '3'
tasks:
  build:
    sources: [in.txt]
    generates: [out.txt]
    cmds: ['$cmd']
EOF
}

V1="echo ran >> runs.log && cp in.txt out.txt"
V2="echo ran >> runs.log && cp in.txt out.txt && echo edited >> out.txt"

count() { [ -f "$1/runs.log" ] && wc -l < "$1/runs.log" | tr -d ' ' || echo 0; }

probe() {
  local name="$1"; shift
  local dir="$work/$name"
  rm -rf "$dir" && mkdir -p "$dir"
  echo "content" > "$dir/in.txt"

  write_files "$dir" "$V1"
  ( cd "$dir" && "$@" >/dev/null 2>&1 )
  local first; first=$(count "$dir")

  # Identical re-run: a cache should skip it.
  ( cd "$dir" && "$@" >/dev/null 2>&1 )
  local same; same=$(count "$dir")

  # Only the command changes. in.txt is byte-for-byte identical.
  write_files "$dir" "$V2"
  ( cd "$dir" && "$@" >/dev/null 2>&1 )
  local edited; edited=$(count "$dir")

  local cached verdict
  [ "$same" -eq "$first" ] && cached="yes" || cached="no"
  if [ "$edited" -gt "$same" ]; then verdict="re-runs"; else verdict="STALE"; fi

  printf '| %-8s | %-15s | %-25s |\n' "$name" "$cached" "$verdict"
}

echo
echo "| runner   | caches a re-run | after the command changes |"
echo "|----------|-----------------|---------------------------|"
probe aurora "$AURORA" build --no-tui
probe make   "$MAKE" out.txt
probe just   just build
probe task   task build
echo
echo "'STALE' means the runner skipped the task and left an output its current"
echo "definition would never have produced."
