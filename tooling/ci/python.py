"""Whether `python3` can run Dispatch's tooling and tests: Python 3.11 or later, for `tomllib`.

`./setup`, `dispatchdev doctor` and `dispatchdev check`, `test` and `verify` ask it first, so an
older `python3`, such as the one macOS ships, stops them with what to do, not a failure deep in a
check. It runs on any Python 3, older ones included, to say so.
"""
import sys

NEEDED = (3, 11)

if sys.version_info < NEEDED:
    sys.exit(
        f"python3 is Python {sys.version.split()[0]}, at {sys.executable}; Dispatch needs "
        f"{NEEDED[0]}.{NEEDED[1]} or later. Install one and put it first on PATH as python3: "
        "Homebrew's is in $(brew --prefix)/opt/python@3.<minor>/libexec/bin."
    )
