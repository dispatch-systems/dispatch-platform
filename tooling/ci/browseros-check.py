#!/usr/bin/env python3
"""Isolated native suites, a shard at a time. The capacity shard gets its own otherwise idle
runner, and so does `host`, the Rust probe of the browser's isolation on this machine, which
compiles its own test program."""

import argparse
import json
import os
from pathlib import Path
import subprocess

# The one list of native suites, shared with the check that no test file is left out.
PLAN = json.loads((Path(__file__).resolve().parent / "test-plan.json").read_text())
SHARDS = PLAN["native"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shard", choices=["all", "host", *SHARDS], default="all")
    args = parser.parse_args()
    shard = args.shard
    root = Path(__file__).resolve().parents[2]
    environment = dict(os.environ)
    environment.setdefault("DISPATCH_BWRAP_EXECUTABLE", "/usr/local/libexec/dispatch-dev/bwrap")
    subprocess.run(["tooling/cli/dispatchdev", "build"], cwd=root, env=environment, check=True)
    if shard in ("all", "host"):
        subprocess.run([
            "cargo", "test", "--locked", "--test", "browseros_host", "--",
            "--ignored", "--nocapture", "--test-threads=1",
        ], cwd=root, env=environment, check=True)
    if shard == "host":
        return
    environment["DISPATCH_TEST_NATIVE"] = "1"
    files = [file for group in SHARDS.values() for file in group] if shard == "all" else SHARDS[shard]
    subprocess.run([
        "node", "node_modules/tsx/dist/cli.mjs", "--test", "--test-concurrency=1", *files,
    ], cwd=root, env=environment, check=True)


if __name__ == "__main__":
    main()
