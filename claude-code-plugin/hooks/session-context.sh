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
  # Regex extraction (not a full JSON parser); benign fallback to CLAUDE_PROJECT_DIR/pwd.
  cwd="$(printf '%s' "$input" \
    | sed -n 's/.*"cwd"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n1)"
fi
[ -n "$cwd" ] || cwd="$(pwd)"

# No Beamfile -> not an Aurora project, stay silent.
[ -f "$cwd/Beamfile" ] || exit 0

if command -v aurora >/dev/null 2>&1; then
  # Fetch the beam list; empty on failure (cd or aurora). Explicit
  # form rather than `&& ... || true` to stay unambiguous (SC2015).
  beams="$(cd "$cwd" && aurora --list 2>/dev/null)" || beams=""
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
  # Strip remaining raw control bytes (U+0000 to U+001F, excluding the
  # \n, \r, \t already escaped) to guarantee a valid JSON additionalContext.
  s="$(printf '%s' "$s" | LC_ALL=C tr -d '\000-\010\013\014\016-\037')"
  printf '%s' "$s"
}

escaped="$(escape_for_json "$context")"
printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$escaped"

exit 0
