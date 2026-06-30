#!/usr/bin/env bash
# PostToolUse hook: validate a Beamfile right after it is edited.
# Reads the hook event JSON on stdin, extracts the edited file path, and if it
# is a Beamfile runs `aurora --dry-run` to parse it and resolve the DAG.
# Degrades gracefully: if aurora is not installed, exits 0 without blocking.

set -euo pipefail

input="$(cat)"

# Extract the edited file path from the tool input JSON.
# Extraction regex (pas un parseur JSON complet) ; en cas d'échec, le fallback est bénin (on ne valide pas).
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
