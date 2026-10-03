#!/usr/bin/env python3
"""Compatibility launcher for Rust build caching and compiler fingerprints."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "ci"))
from ci_tool import launch  # noqa: E402


def main(argv=None):
    launch("build", *(sys.argv[1:] if argv is None else argv))


if __name__ == "__main__":
    main()
