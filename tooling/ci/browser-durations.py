#!/usr/bin/env python3
"""Refresh browser-durations.json, the time each browser test takes in the merge queue.

The browser shards split the suite by these times, so each shard takes about as long as the
others. Each entry is the median over the newest passing queue runs, read from the browser
jobs' logs. A test missing from the file counts as the median of the rest, so the file only
needs refreshing when the split drifts: `python3 tooling/ci/browser-durations.py [runs]`.
"""
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys

REPOSITORY = "dispatch-systems/dispatch-platform"
OUTPUT = Path(__file__).resolve().parent / "browser-durations.json"
# The list reporter's line for a passing test, as
# `✓  12 features/team/tests/browser/roles.spec.ts:7:1 › describe › title (3.4s)`.
PASSED = re.compile(r"✓\s+\d+ ((?:[\w.-]+/)*tests/browser/[^:\s]+):\d+:\d+ › (.+) \((\d+(?:\.\d+)?)(ms|s|m)\)\s*$")
UNITS = {"ms": 0.001, "s": 1, "m": 60}


def api(path):
    return json.loads(subprocess.run(["gh", "api", path], check=True, capture_output=True,
                                     text=True).stdout)


def times(log):
    """Seconds per `file › title` in one browser job's log."""
    found = {}
    for line in log.splitlines():
        match = PASSED.search(line)
        if match:
            file, title, value, unit = match.groups()
            found[f"{file} › {title}"] = float(value) * UNITS[unit]
    return found


def main(argv=None):
    count = int((argv if argv is not None else sys.argv[1:] or ["10"])[0])
    runs = api(f"repos/{REPOSITORY}/actions/workflows/checks.yml/runs"
               f"?event=merge_group&status=success&per_page={count}")["workflow_runs"]
    samples = {}
    for run in runs:
        jobs = api(f"repos/{REPOSITORY}/actions/runs/{run['id']}/jobs?per_page=100")["jobs"]
        for job in jobs:
            if not job["name"].startswith("browser ("):
                continue
            log = subprocess.run(["gh", "run", "view", "--repo", REPOSITORY, "--job", str(job["id"]),
                                  "--log"], check=True, capture_output=True, text=True).stdout
            for test, seconds in times(log).items():
                samples.setdefault(test, []).append(seconds)
    if not samples:
        raise SystemExit("No passing browser tests found in the queue runs' logs")
    medians = {test: round(statistics.median(values), 1) for test, values in sorted(samples.items())}
    OUTPUT.write_text(json.dumps(medians, indent=2, ensure_ascii=False) + "\n")
    print(f"{len(medians)} browser tests from {len(runs)} queue runs written to {OUTPUT}")


if __name__ == "__main__":
    main()
