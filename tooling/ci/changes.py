#!/usr/bin/env python3
"""Which of the platform checks' suites a pull request needs (`.github/workflows/checks.yml`).

A suite runs unless every file the PR changes is of a kind it never reads:

- `android`: the Android apps, a Gradle project nothing else builds or reads. Their own job runs
  only for them and for CI's own files.
- `docs`: Markdown outside `services/`, whose skills the support worker ships. The rules in the
  api job and the generators in the scaffold job read Markdown, so those two still run.
- `frontend`: a `frontend/` folder's code, which no collector's native test reaches and no Rust
  reads, but for the owners' manifests (`frontend/feature.ts`) and the list of them
  (`app/frontend/features.ts`), which the backend's export test writes.
- Anything else, CI's own files included, is read by every suite.

Outside a pull request every suite runs: `main`'s run of each commit is complete, and Dev
installs only what it passed. The Android apps' job alone still follows a push's change: the apps
ship only in a release, which builds and tests them again. Every suite runs whenever the change
can't be read: this fails open. A pull request's checkout is GitHub's merge commit and a push's
is the commit itself, so the first parent is the base either changes.

It also says whether the debug backend and worker need building for the run (`debug`): only when
a PR changes what they compile from, as `dispatchdev build`'s key reads it
(`tooling/cli/src/build/key.rs`). Otherwise `main`'s build, which `main`'s own run saves, is the
same, and the jobs that use it restore it without waiting. Should this list fall behind the key's,
a job finds no build of its inputs and builds its own: slower, never wrong.

Prints `<suite>=true|false` for each suite, `debug=true|false`, and `changed=<kinds>` for
`$GITHUB_OUTPUT`.
"""
from __future__ import annotations

import subprocess
import sys

# Each suite but android's, and the kinds of file it never reads.
INERT = {
    "core": {"android", "docs", "frontend"},
    "api": {"android"},
    "scaffold": {"android"},
    "collectors": {"android", "docs", "frontend"},
    "browser": {"android", "docs"},
}
SUITES = [*INERT, "android"]
# What the package's debug build compiles from: the workspace's files and its crates' folders, but
# frontend code, tests, and the development CLI's own entry point and launcher.
RUST_FILES = {
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rust-toolchain",
    "tooling/build/rustc-remap.py",
    "ops/launchers/runtime_artifact.py",
    "ops/launchers/update-dev.py",
    "ops/launchers/update-production.py",
    "ops/launchers/support-collection.py",
    "ops/systemd/dispatch-support-worker.service",
}
RUST_ROOTS = (
    "app/", "core/", "collectors/", "features/", "mcp/", "ops/host-manager/",
    "services/support-worker/", "tooling/cli/", "tooling/shared/", ".cargo/",
)


def kind(path: str) -> str:
    if path.startswith("android/"):
        return "android"
    if path.startswith(".github/") or path == "tooling/ci/changes.py":
        return "ci"
    if path.endswith(".md") and not path.startswith("services/"):
        return "docs"
    folders, name = path.split("/")[:-1], path.split("/")[-1]
    if "frontend" in folders and not (
        (folders[-1] == "frontend" and name == "feature.ts") or path == "app/frontend/features.ts"
    ):
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


def compiled(path: str) -> bool:
    """Whether the debug backend and worker compile from `path`."""
    parts = path.split("/")
    if path in RUST_FILES:
        return True
    return (
        path.startswith(RUST_ROOTS)
        and not {"frontend", "node_modules", "tests"} & set(parts[:-1])
        and not path.endswith((".ts", ".tsx", ".mts", ".cts", ".css"))
        and path not in ("tooling/cli/src/main.rs", "tooling/cli/dispatchdev")
    )


def rebuilds(paths: list[str] | None, pull_request: bool) -> bool:
    """Whether the run builds the debug backend and worker: outside a PR always, to save `main`'s;
    on one, when it changes what they compile from, or when its change is unknown."""
    return not pull_request or not paths or any(compiled(path) for path in paths)


def changed() -> list[str] | None:
    """The paths the checked-out commit changes, or None when git can't say."""
    # NUL-separated, so git writes each path as it is rather than quoting an unusual one.
    done = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "-z", "HEAD^1", "HEAD"],
        capture_output=True, text=True,
    )
    if done.returncode != 0:
        print(f"Could not read the change, so every suite runs: {done.stderr.strip()}", file=sys.stderr)
        return None
    return [path for path in done.stdout.split("\0") if path]


def main(argv: list[str]) -> None:
    event = argv[1] if len(argv) > 1 else ""
    paths = changed() if event in ("pull_request", "push") else None
    suites, kinds = needed(paths, pull_request=event == "pull_request")
    for suite in SUITES:
        print(f"{suite}={'true' if suites[suite] else 'false'}")
    print(f"debug={'true' if rebuilds(paths, event == 'pull_request') else 'false'}")
    print(f"changed={' '.join(kinds) if kinds else 'all'}")


if __name__ == "__main__":
    main(sys.argv)
