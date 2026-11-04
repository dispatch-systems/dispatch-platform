#!/usr/bin/env python3
"""Deletes `main`'s caches that newer ones of the same kind replaced, and closed PRs' caches
(`caches.yml`).

GitHub keeps 10 GB of caches a repository and past that evicts the least recently used, whatever
they hold. Each kind below is saved again whenever its inputs change, and only its newest entries
are read: Cargo's dependency caches, of this week's generation and the current lockfile; the
pinned Playwright browser and BrowserOS package; the installed packages and npm's download cache
they install from; the pinned Rust toolchain; CodeQL's overlay database, which only a pull
request's analysis would read, while CodeQL analyses `main` alone; the CI tools and assessment
fixture; the exact release and debug backend and worker; and the scaffold jobs' sccache, whose
newest alone stays, whatever its last read: each main run restores the one before it, which then
looks in use, and a day's would all stay. The two newest of each other kind stay, so a branch still on the lockfile
`main` just left keeps its cache; older ones only take room `main`'s current caches need. The tools, fixture and executables keep
more, keyed by sources a branch may still have; the debug ones, the largest, four. Past those, an
entry anything read in the last day stays too: `main` back on an older lockfile, or an open PR on
one, still uses it. CodeQL's are the exception: `main` only writes them, and GitHub counts the
write as a read, so a day's analyses, one a push, would all stay for a day. The executables'
keys before they named their profile keep none: nothing looks them up any more. A key of no kind
below is never touched.

A PR's runs save caches only it reads, under its merge ref. `pr-caches.yml` deletes them when it
closes; this deletes any its runs saved after that, once GitHub answers that it is closed.

- `prune [--dry-run]`: deletes each kind's older entries on `main`, and closed PRs' caches, a line
  for each.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
from datetime import datetime, timedelta, timezone
import json
import os
import re
import subprocess
import sys
from typing import Callable, NamedTuple

REF = "refs/heads/main"
PULL = re.compile(r"refs/pull/(\d+)/merge")
# An entry read this recently is in use, whatever its kind's newer entries.
RECENT = timedelta(days=1)
OS = r"([A-Za-z0-9]+-[A-Za-z0-9]+)"
# Each kind's pattern, its groups naming what makes two entries the same kind, how many of it
# stay, and whether anything reads its older entries: only then does a recent read keep one.
KINDS = {
    "rust": (re.compile(rf"v\d+-rust-\d{{4}}-W\d{{2}}-(dispatch-[a-z]+)-{OS}-[0-9a-f]+-[0-9a-f]+"), 2, True),
    "playwright": (re.compile(rf"dispatch-playwright-{OS}-[0-9.]+"), 2, True),
    "browseros": (re.compile(r"dispatch-browseros-[0-9a-f]+"), 2, True),
    "packages": (re.compile(rf"dispatch-node-modules-{OS}-[0-9a-f]+"), 2, True),
    "downloads": (re.compile(rf"dispatch-npm-{OS}-[0-9a-f]+"), 2, True),
    "toolchain": (re.compile(rf"dispatch-rust-toolchain-{OS}-[0-9a-f]+"), 2, True),
    "codeql": (re.compile(r"codeql-overlay-base-database-\d+-[0-9a-f]+-([a-z]+)-[0-9.]+-[0-9a-f]+-\d+-\d+"), 2, False),
    "tools": (re.compile(rf"dispatch-(tools|fixture)-{OS}-([a-z0-9]+)-[0-9a-f]+"), 10, True),
    "binaries": (re.compile(rf"dispatch-rust-binary-release-{OS}-[0-9a-f]+"), 10, True),
    "debug binaries": (re.compile(rf"dispatch-rust-binary-debug-{OS}-[0-9a-f]+"), 4, True),
    "unnamed binaries": (re.compile(rf"dispatch-rust-binary-{OS}-[0-9a-f]+"), 0, False),
    "sccache": (re.compile(rf"dispatch-sccache-([a-z-]+)-{OS}-[0-9]+"), 1, False),
}

Run = Callable[[list], str]


class Cache(NamedTuple):
    id: int
    key: str
    ref: str
    created: str
    accessed: str


def moment(text: str) -> datetime:
    return datetime.fromisoformat(text.replace("Z", "+00:00"))


def kind(key: str) -> tuple[str, int, bool] | None:
    """The kind `key` is one entry of, with what tells its entries apart, how many of it stay,
    and whether its older entries are read; None for any other key."""
    for name, (pattern, keep, read) in KINDS.items():
        match = pattern.fullmatch(key)
        if match:
            return " ".join([name, *match.groups()]), keep, read
    return None


def superseded(caches: list[Cache], now: datetime) -> list[Cache]:
    """`main`'s entries of each kind past the newest it keeps and, for a kind anything reads,
    unread for a day, oldest first."""
    kinds = defaultdict(list)
    for cache in caches:
        found = kind(cache.key)
        if cache.ref == REF and found:
            kinds[found].append(cache)
    older = [
        cache
        for (_, keep, read), entries in kinds.items()
        for cache in sorted(entries, key=lambda each: each.created, reverse=True)[keep:]
        if not read or now - moment(cache.accessed) > RECENT
    ]
    return sorted(older, key=lambda each: each.created)


def closed(caches: list[Cache], state: Callable[[int], str | None]) -> list[Cache]:
    """The caches of PRs whose state is `closed`; a PR whose state is unknown keeps its own."""
    numbers = {int(match[1]) for cache in caches if (match := PULL.fullmatch(cache.ref))}
    shut = {number for number in numbers if state(number) == "closed"}
    return [
        cache for cache in caches
        if (match := PULL.fullmatch(cache.ref)) and int(match[1]) in shut
    ]


def run(args: list) -> str:
    done = subprocess.run(args, capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"gh failed: {done.stderr.strip()}")
    return done.stdout


def listed(repository: str, run: Run = run) -> list[Cache]:
    """Every ref's caches. Read as JSON, since a PR's workflows choose its caches' keys."""
    pages = json.loads(run([
        "gh", "api", "--paginate", "--slurp", f"repos/{repository}/actions/caches?per_page=100",
    ]))
    return [
        Cache(int(cache["id"]), cache["key"], cache["ref"], cache["created_at"],
              cache["last_accessed_at"])
        for page in pages
        for cache in page["actions_caches"]
    ]


def prune(repository: str, dry_run: bool, run: Run = run) -> None:
    def state(number: int) -> str | None:
        try:
            return run(["gh", "api", f"repos/{repository}/pulls/{number}", "--jq", ".state"]).strip()
        except RuntimeError:
            return None

    caches = listed(repository, run)
    for cache in [*superseded(caches, datetime.now(timezone.utc)), *closed(caches, state)]:
        if not dry_run:
            try:
                run(["gh", "api", "-X", "DELETE", f"repos/{repository}/actions/caches/{cache.id}"])
            except RuntimeError as error:
                # GitHub may have evicted it since it was listed.
                if "HTTP 404" not in str(error):
                    raise
        print(f"{'Would delete' if dry_run else 'Deleted'} {cache.id} on {cache.ref} ({cache.created})")


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    pruning = commands.add_parser("prune")
    pruning.add_argument("--dry-run", action="store_true")
    pruning.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY"))
    args = parser.parse_args(argv)
    if not args.repository:
        sys.exit("No repository: pass --repository or set GITHUB_REPOSITORY")
    prune(args.repository, args.dry_run)


if __name__ == "__main__":
    main()
