#!/usr/bin/env python3
"""`npm audit --audit-level=high`, less the advisories listed here: `npm-audit.py` runs the audit
and fails on any high or critical advisory, unless that advisory on that exact version is allowed
below with why it stays. An allowed entry the audit no longer finds fails it too, so each entry
leaves with the dependency that needed it."""
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).parents[2]
DENIED = {"high", "critical"}

# Advisories the lockfile may hold, each on one exact version, with why it is safe and when it
# goes.
ALLOWED_ADVISORIES = {}


def problems(report, versions, advisories=ALLOWED_ADVISORIES):
    """What fails the audit, from npm audit's JSON `report` and the lockfile's `versions`, each
    installed path's. A package named only for depending on another's advisory adds nothing:
    the advisory is counted where it is."""
    found = []
    seen = set()
    for entry in report["vulnerabilities"].values():
        for via in entry["via"]:
            if isinstance(via, str) or via["severity"] not in DENIED:
                continue
            id = via["url"].rsplit("/", 1)[-1]
            for node in entry["nodes"]:
                advisory = (id, via["name"], versions[node])
                if advisory in advisories:
                    seen.add(advisory)
                elif (line := f"{advisory[0]}: {advisory[1]} {advisory[2]}") not in found:
                    found.append(line)
    found += [
        f"{id} on {name} {version} is allowed but no longer found: remove it from "
        "ALLOWED_ADVISORIES"
        for id, name, version in advisories
        if (id, name, version) not in seen
    ]
    return found


def main():
    run = subprocess.run(["npm", "audit", "--json"], capture_output=True, text=True, cwd=ROOT)
    try:
        report = json.loads(run.stdout)
        report["vulnerabilities"]
    except (json.JSONDecodeError, KeyError):
        sys.stderr.write(run.stderr)
        sys.exit(f"npm audit gave no report (exit {run.returncode})")
    packages = json.loads((ROOT / "package-lock.json").read_text())["packages"]
    versions = {path: package.get("version", "") for path, package in packages.items()}
    for (id, name, version), reason in ALLOWED_ADVISORIES.items():
        print(f"Allowed {id} on {name} {version}: {reason}")
    found = problems(report, versions)
    if found:
        sys.exit("npm audit denies:\n" + "\n".join(f"- {line}" for line in found))
    print("npm audit denies nothing.")


if __name__ == "__main__":
    main()
