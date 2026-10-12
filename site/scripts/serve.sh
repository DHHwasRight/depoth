#!/usr/bin/env bash
#
# Builds the site and serves it, in the foreground.
#
# Playwright owns this process: it starts it, waits for the port, and kills it
# when the run ends. So the server must not daemonise, and the build must
# finish before the port opens -- otherwise Playwright starts testing an empty
# directory. Both are why this is one script and not two.
#
# The static server is python3's, which is on every machine this project
# targets. A node server would add a dependency to a suite that already has
# one.
#
# Environment:
#   PORT=4317        the port to serve on; must match playwright.config.ts
#   DIST=<path>      serve this directory instead of site/dist
#   SKIP_BUILD=1     serve what is already built, and build nothing
set -euo pipefail

PORT="${PORT:-4317}"
HOST="${HOST:-127.0.0.1}"

site_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_dir="$(dirname "$site_dir")"
dist="${DIST:-$site_dir/dist}"

if [ "${SKIP_BUILD:-}" != "1" ]; then
  # Tailwind first: the Rust build copies assets, it does not compile CSS, so
  # a site built before this ran carries the previous stylesheet. Every layout
  # and theme claim in the suite is a claim about that file.
  if [ -x "$site_dir/scripts/build-css.sh" ]; then
    bash "$site_dir/scripts/build-css.sh"
  fi

  # `--no-partials` because a fragment is fetched, never navigated to: a
  # browser suite only ever visits complete documents, and the Rust harness
  # already owns the rule that fragments must not be reachable.
  cd "$repo_dir"
  mise exec -- cargo run -q -p depoth-site -- build --no-partials --out site/dist
fi

# Fail here with the real reason. Without this the next line fails with a bare
# "cd: no such file or directory", which reads as a broken script rather than
# as a build that wrote nothing.
if [ ! -f "$dist/index.html" ]; then
  echo "error: no $dist/index.html after the build." >&2
  echo "       the build ran but wrote no pages, so there is nothing to test." >&2
  exit 1
fi

cd "$dist"
# exec, so signals from Playwright reach python rather than this shell.
exec python3 -m http.server "$PORT" --bind "$HOST"
