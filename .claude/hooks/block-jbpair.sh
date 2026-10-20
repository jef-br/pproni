#!/usr/bin/env bash
# PreToolUse hook on Bash: block the AGENT's `cargo build` while any <jbpair ...>
# review tag remains in src/.
#
# Why a hook and not build.rs: a hook only sees the agent's tool calls, so your
# own manual `cargo build` in a terminal is never blocked (your override stays).
# <jbcomplete> tags are NOT matched here, so a resolved region does not block.
# docs/ and build.rs are never scanned (only src/), so example tags in the
# knowledge files can't trip this.
set -euo pipefail

input="$(cat)"

# Only act on a cargo build (any form, including rtk cargo build / --release).
case "$input" in
  *cargo*build*) : ;;
  *) exit 0 ;;
esac

root="${CLAUDE_PROJECT_DIR:-.}"
hits="$(grep -rIl --include='*.rs' '<jbpair' "$root/src" 2>/dev/null || true)"
if [ -n "$hits" ]; then
  echo "BLOCKED: unresolved <jbpair> review tags remain in src/:" >&2
  echo "$hits" | sed 's#^#  - #' >&2
  echo "Resolve each (swap <jbpair> -> <jbcomplete>) or strip it before building." >&2
  exit 2
fi
exit 0
