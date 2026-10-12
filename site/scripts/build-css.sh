#!/usr/bin/env bash
#
# Build assets/css/site.css from assets/css/_tailwind.css.
#
# Usage:
#   scripts/build-css.sh            # build once, minified
#   scripts/build-css.sh --watch    # build again on each change
#
# The script runs the Tailwind CLI from node_modules. It does not fall back to
# `npx`, because npx downloads a package when the local one is absent, and a
# silent download hides the real fault: nobody ran `npm install`.

set -euo pipefail

# The site root is the parent of the directory that holds this script.
SITE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SITE_ROOT"

INPUT="assets/css/_tailwind.css"
OUTPUT="assets/css/site.css"
CLI="node_modules/.bin/tailwindcss"

if [ ! -x "$CLI" ]; then
  echo "error: the Tailwind CLI is absent at $SITE_ROOT/$CLI" >&2
  echo "       run 'npm install' in $SITE_ROOT first." >&2
  exit 1
fi

if [ ! -f "$INPUT" ]; then
  echo "error: the build input is absent at $SITE_ROOT/$INPUT" >&2
  exit 1
fi

# _tailwind.css imports these two. An absent import stops the build, so make
# an empty one. Another agent owns their content.
for part in docs landing; do
  if [ ! -f "assets/css/$part.css" ]; then
    echo "note: assets/css/$part.css was absent; wrote an empty placeholder." >&2
    printf '/* %s.css — placeholder. */\n' "$part" > "assets/css/$part.css"
  fi
done

if [ "${1:-}" = "--watch" ]; then
  exec "$CLI" --input "$INPUT" --output "$OUTPUT" --watch
fi

"$CLI" --input "$INPUT" --output "$OUTPUT" --minify

if [ ! -s "$OUTPUT" ]; then
  echo "error: the build wrote no CSS to $SITE_ROOT/$OUTPUT" >&2
  exit 1
fi

echo "built $OUTPUT ($(wc -c < "$OUTPUT" | tr -d ' ') bytes)"
