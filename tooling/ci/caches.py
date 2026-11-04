#!/usr/bin/env python3
"""Deletes `main`'s caches that newer ones of the same kind replaced (`caches.yml`).

GitHub keeps 10 GB of caches a repository and past that evicts the least recently used, whatever
they hold. Each kind below is saved again whenever its inputs change, and only its newest entries
are read: Cargo's dependency caches, of this week's generation and the current lockfile; the
pinned Playwright browser and BrowserOS package; the installed packages; CodeQL's overlay
database, which only a pull request's analysis would read, while CodeQL analyses `main` alone;
and the CI tools and assessment fixture. The two newest of each kind stay, so a branch still on
the lockfile `main` just left keeps its cache; older ones only take room `main`'s current caches
need. The tools and fixture keep ten, being small and keyed by sources a branch may still have.
A key of no kind below is never touched.

- `prune [--dry-run]`: deletes each kind's older entries on `main`, a line for each.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
import os
import re
import subprocess
import sys
from typing import Callable, NamedTuple

REF = "refs/heads/main"
OS = r"([A-Za-z0-9]+-[A-Za-z0-9]+)"
# Each kind's pattern, its groups naming what makes two entries the same kind, and how many of it
# stay.
KINDS = {
    "rust": (re.compile(rf"v\d+-rust-\d{{4}}-W\d{{2}}-(dispatch-[a-z]+)-{OS}-[0-9a-f]+-[0-9a-f]+"), 2),
    "playwright": (re.compile(rf"dispatch-playwright-{OS}-[0-9.]+"), 2),
    "browseros": (re.compile(r"dispatch-browseros-[0-9a-f]+"), 2),
    "packages": (re.compile(rf"dispatch-node-modules-{OS}-[0-9a-f]+"), 2),
    "codeql": (re.compile(r"codeql-overlay-base-database-\d+-[0-9a-f]+-([a-z]+)-[0-9.]+-[0-9a-f]+-\d+-\d+"), 2),
    "tools": (re.compile(rf"dispatch-(tools|fixture)-{OS}-([a-z0-9]+)-[0-9a-f]+"), 10),
}

Run = Callable[[list], str]


class Cache(NamedTuple):
    id: int
    key: str
    created: str


def kind(key: str) -> tuple[str, int] | None:
    """The kind `key` is one entry of, with what tells its entries apart, and how many of it
    stay; None for any other key."""
    for name, (pattern, keep) in KINDS.items():
        match = pattern.fullmatch(key)
        if match:
            return " ".join([name, *match.groups()]), keep
    return None


def superseded(caches: list[Cache]) -> list[Cache]:
    """The entries of each kind past the newest it keeps, oldest first."""
    kinds = defaultdict(list)
    for cache in caches:
        found = kind(cache.key)
        if found:
            kinds[found].append(cache)
    older = [
        cache
        for (_, keep), entries in kinds.items()
        for cache in sorted(entries, key=lambda each: each.created, reverse=True)[keep:]
    ]
    return sorted(older, key=lambda each: each.created)


def run(args: list) -> str:
    done = subprocess.run(args, capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"gh failed: {done.stderr.strip()}")
    return done.stdout


def listed(repository: str, run: Run = run) -> list[Cache]:
    """`main`'s caches."""
    out = run([
        "gh", "api", "--paginate",
        f"repos/{repository}/actions/caches?ref={REF}&per_page=100",
        "--jq", ".actions_caches[] | [.id, .key, .created_at] | @tsv",
    ])
    return [
        Cache(int(id), key, created)
        for id, key, created in (line.split("\t") for line in out.splitlines() if line)
    ]


def prune(repository: str, dry_run: bool, run: Run = run) -> None:
    for cache in superseded(listed(repository, run)):
        if not dry_run:
            try:
                run(["gh", "api", "-X", "DELETE", f"repos/{repository}/actions/caches/{cache.id}"])
            except RuntimeError as error:
                # GitHub may have evicted it since it was listed.
                if "HTTP 404" not in str(error):
                    raise
        print(f"{'Would delete' if dry_run else 'Deleted'} {cache.key} ({cache.created})")


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
