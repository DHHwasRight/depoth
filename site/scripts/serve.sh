#!/usr/bin/env bash
#
# Builds the site and serves it, in the foreground.
#
# Playwright owns this process: it starts it, waits for the port, and kills it
# when the run ends. So the server must not daemonise, and the build must
# happen before the port opens -- otherwise Playwright starts testing an empty
# directory. Both are why this is one script and not two.
#
# The static server is python3's, which is on every machine this project
# targets. Adding a node server would add a dependency to a suite that already
# has one.
set -euo pipefail

PORT="${PORT:-4317}"
HOST="${HOST:-127.0.0.1}"

site_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_dir="$(dirname "$site_dir")"

# `--no-partials` because a fragment is fetched, never navigated to: a browser
# suite only ever visits complete documents, and the Rust harness already owns
# the rule that fragments must not be reachable.
cd "$repo_dir"
mise exec -- cargo run -q -p depoth-site -- build --no-partials --out site/dist

cd "$site_dir/dist"
# exec, so signals from Playwright reach python rather than this shell.
exec python3 -m http.server "$PORT" --bind "$HOST"
