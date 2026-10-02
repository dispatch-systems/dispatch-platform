#!/usr/bin/env python3
"""Isolated native suites; the capacity shard gets its own otherwise idle runner."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import re
import sys
import xml.etree.ElementTree as ET

# The one list of native suites, shared with the check that no test file is left out.
PLAN = json.loads((Path(__file__).resolve().parent / "ci/test-plan.json").read_text())
SHARDS = PLAN["native"]
REAL_TIMEOUT = PLAN["nativeRealTimeout"]


def require_real_timeout(report):
    """A zero exit with the sentinel absent or skipped is not timeout coverage."""
    try:
        cases = [case for case in ET.fromstring(report).iter("testcase")
                 if case.get("name") == REAL_TIMEOUT["title"]]
    except ET.ParseError as error:
        raise SystemExit("Native timeout runner did not produce its JUnit result") from error
    if len(cases) != 1 or any(cases[0].find(tag) is not None for tag in ["skipped", "failure", "error"]):
        raise SystemExit("Native timeout sentinel must execute and pass exactly once")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shard", choices=["all", *SHARDS], default="all")
    parser.add_argument("--host-only", action="store_true")
    parser.add_argument("--real-timeouts", action="store_true",
                        help="Run only the real wall-clock timeout sentinel and require it to pass")
    args = parser.parse_args()
    shard = args.shard
    if args.real_timeouts and (args.host_only or shard != "all"):
        parser.error("--real-timeouts runs only its sentinel; do not combine it with other selectors")
    root = Path(__file__).resolve().parent.parent
    environment = dict(os.environ)
    environment.setdefault("DISPATCH_BWRAP_EXECUTABLE", "/usr/local/libexec/dispatch-dev/bwrap")
    environment.pop("DISPATCH_TEST_REAL_TIMEOUTS", None)
    subprocess.run(["python3", "tooling/cargo-build.py"], cwd=root, env=environment, check=True)
    if not args.real_timeouts and (args.host_only or shard in ("all", "capacity")):
        subprocess.run([
            "cargo", "test", "--locked", "--test", "browseros_host", "--",
            "--ignored", "--nocapture", "--test-threads=1",
        ], cwd=root, env=environment, check=True)
    if args.host_only:
        return
    environment["DISPATCH_TEST_NATIVE"] = "1"
    if args.real_timeouts:
        environment["DISPATCH_TEST_REAL_TIMEOUTS"] = "1"
        result = subprocess.run([
            "node", "node_modules/tsx/dist/cli.mjs", "--test", "--test-concurrency=1",
            "--test-reporter=junit", "--test-name-pattern=^" + re.escape(REAL_TIMEOUT["title"]) + "$",
            REAL_TIMEOUT["file"],
        ], cwd=root, env=environment, capture_output=True, text=True)
        print(result.stdout, end="")
        print(result.stderr, end="", file=sys.stderr)
        result.check_returncode()
        require_real_timeout(result.stdout)
        return
    files = [file for group in SHARDS.values() for file in group] if shard == "all" else SHARDS[shard]
    subprocess.run([
        "node", "node_modules/tsx/dist/cli.mjs", "--test", "--test-concurrency=1", *files,
    ], cwd=root, env=environment, check=True)


if __name__ == "__main__":
    main()
