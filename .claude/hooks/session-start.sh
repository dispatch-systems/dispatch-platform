#!/usr/bin/env bash
# Gets a Claude Code cloud session ready to build, test and lint: the pinned Rust toolchain,
# the crates in Cargo.lock and the npm packages in package-lock.json. Runs only in cloud
# sessions (CLAUDE_CODE_REMOTE=true), so local checkouts and CI never run it. A step that
# fails is reported and the session still starts.
set -uo pipefail

if [[ ${CLAUDE_CODE_REMOTE:-} != true ]]; then
  exit 0
fi
cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}" || exit 0

status=0

# rust-toolchain.toml makes cargo install its toolchain first; the scaffold rule tests run
# `cargo update --offline`, which fails until the registry index and crates are here.
if ! cargo fetch --locked --quiet; then
  echo "session-start: cargo fetch failed" >&2
  status=1
fi

# npm ci only when package-lock.json changed since the last install, so a resumed session
# starts at once and the lockfile is never rewritten.
stamp=node_modules/.cache/dispatch/session-start-lock.sha256
lock=$(sha256sum package-lock.json | cut -d' ' -f1)
if [[ ! -d node_modules || $(cat "$stamp" 2>/dev/null) != "$lock" ]]; then
  if npm ci --ignore-scripts --no-audit --no-fund --loglevel=error >&2; then
    mkdir -p "$(dirname "$stamp")" && echo "$lock" >"$stamp"
  else
    echo "session-start: npm ci failed" >&2
    status=1
  fi
fi

if [[ $status != 0 ]]; then
  echo "session-start: setup was incomplete; rerun .claude/hooks/session-start.sh" >&2
fi
exit 0
