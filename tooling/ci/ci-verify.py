#!/usr/bin/env python3
"""The gate job's artifact check: `ci-verify.py <archive>` unpacks the packaged build and has the
host manager verify its inventory and source commit against the run's own commit."""
import argparse
import os
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "ops/launchers"))
from runtime_artifact import host_binary  # noqa: E402


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive")
    args = parser.parse_args(argv)
    binary = str(host_binary())
    os.execv(binary, [binary, "host", "ci", "verify", args.archive])


if __name__ == "__main__":
    main()
