#!/usr/bin/env python3
"""Which of the platform checks' suites a pull request needs (`.github/workflows/checks.yml`).

A suite runs unless every file the PR changes is of a kind it never reads:

- `android`: the Android apps, a Gradle project nothing else builds or reads. Their own job runs
  only for them and for CI's own files.
- `docs`: Markdown outside `services/`, whose skills the support worker ships. The rules in the
  api job and the generators in the scaffold job read Markdown, so those two still run.
- `frontend`: a `frontend/` folder's code, which no collector's native test reaches.
- Anything else, CI's own files included, is read by every suite.

Outside a pull request every suite runs: `main`'s run of each commit is complete, and Dev
installs only what it passed. The Android apps' job alone still follows a push's change: the apps
ship only in a release, which builds and tests them again. Every suite runs whenever the change
can't be read: this fails open. A pull request's checkout is GitHub's merge commit and a push's
is the commit itself, so the first parent is the base either changes.

Prints `<suite>=true|false` for each suite and `changed=<kinds>` for `$GITHUB_OUTPUT`.
"""
from __future__ import annotations

import subprocess
import sys

# Each suite but android's, and the kinds of file it never reads.
INERT = {
    "core": {"android", "docs"},
    "api": {"android"},
    "scaffold": {"android"},
    "collectors": {"android", "docs", "frontend"},
    "browser": {"android", "docs"},
}
SUITES = [*INERT, "android"]


def kind(path: str) -> str:
    if path.startswith("android/"):
        return "android"
    if path.startswith(".github/") or path == "tooling/ci/changes.py":
        return "ci"
    if path.endswith(".md") and not path.startswith("services/"):
        return "docs"
    if "frontend" in path.split("/")[:-1]:
        return "frontend"
    return "other"


def needed(paths: list[str] | None, pull_request: bool) -> tuple[dict[str, bool], list[str]]:
    """Each suite's answer for a change to `paths`, and the kinds it changed; every suite, and
    no kinds, when `paths` is unknown or empty."""
    kinds = sorted({kind(path) for path in paths or []})
    if not kinds:
        return {suite: True for suite in SUITES}, []
    suites = {
        suite: not pull_request or not set(kinds) <= inert for suite, inert in INERT.items()
    }
    suites["android"] = bool({"android", "ci"} & set(kinds))
    return suites, kinds


def changed() -> list[str] | None:
    """The paths the checked-out commit changes, or None when git can't say."""
    done = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "HEAD^1", "HEAD"],
        capture_output=True, text=True,
    )
    if done.returncode != 0:
        print(f"Could not read the change, so every suite runs: {done.stderr.strip()}", file=sys.stderr)
        return None
    return done.stdout.splitlines()


def main(argv: list[str]) -> None:
    event = argv[1] if len(argv) > 1 else ""
    paths = changed() if event in ("pull_request", "push") else None
    suites, kinds = needed(paths, pull_request=event == "pull_request")
    for suite in SUITES:
        print(f"{suite}={'true' if suites[suite] else 'false'}")
    print(f"changed={' '.join(kinds) if kinds else 'all'}")


if __name__ == "__main__":
    main(sys.argv)
