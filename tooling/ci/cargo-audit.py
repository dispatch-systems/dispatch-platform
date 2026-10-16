#!/usr/bin/env python3
"""`cargo audit --deny warnings`, less the yanked crate versions and advisories listed here:
`cargo-audit.py` runs the audit and fails on any vulnerability or warning, a yanked crate
included, unless that exact version, or that advisory on that exact version, is allowed below
with why it stays. An allowed entry the audit no longer finds fails it too, so each entry leaves
with the dependency that needed it."""
import json
import subprocess
import sys

# Yanked versions the lockfile may hold, each with why it is safe and when it goes.
ALLOWED_YANKED = {}

# Advisories the lockfile may hold, each on one exact version, with why it is safe and when it
# goes.
CALAMINE_ZIP = (
    "only zip 9 fixes it, and calamine, which Cortex reads workbooks with, still needs zip 8 "
    "(tafia/calamine#742). It is about extracting an archive to disk, which neither Dispatch "
    "nor calamine does: both read entries in memory, and Dispatch's own zip is 9. Goes once "
    "calamine moves to zip 9."
)
ALLOWED_ADVISORIES = {
    ("RUSTSEC-2026-0367", "zip", "8.6.0"): CALAMINE_ZIP,
    ("RUSTSEC-2026-0368", "zip", "8.6.0"): CALAMINE_ZIP,
    ("RUSTSEC-2026-0369", "zip", "8.6.0"): CALAMINE_ZIP,
}


def problems(report, allowed=ALLOWED_YANKED, advisories=ALLOWED_ADVISORIES):
    """What fails the audit, from cargo audit's JSON `report`."""
    found = []
    seen_advisories = set()
    for entry in report["vulnerabilities"]["list"]:
        advisory = (entry["advisory"]["id"], entry["package"]["name"], entry["package"]["version"])
        if advisory in advisories:
            seen_advisories.add(advisory)
            continue
        found.append(f"{advisory[0]}: {advisory[1]} {advisory[2]}")
    seen = set()
    for kind, warnings in report["warnings"].items():
        for warning in warnings:
            package = (warning["package"]["name"], warning["package"]["version"])
            if kind == "yanked" and package in allowed:
                seen.add(package)
                continue
            advisory = (warning.get("advisory") or {}).get("id", "")
            found.append(f"{kind}: {package[0]} {package[1]} {advisory}".strip())
    found += [
        f"{name} {version} is allowed yanked but no longer found: remove it from ALLOWED_YANKED"
        for name, version in allowed
        if (name, version) not in seen
    ]
    found += [
        f"{id} on {name} {version} is allowed but no longer found: remove it from "
        "ALLOWED_ADVISORIES"
        for id, name, version in advisories
        if (id, name, version) not in seen_advisories
    ]
    return found


def main():
    run = subprocess.run(["cargo", "audit", "--json"], capture_output=True, text=True)
    try:
        report = json.loads(run.stdout)
    except json.JSONDecodeError:
        sys.stderr.write(run.stderr)
        sys.exit(f"cargo audit gave no report (exit {run.returncode})")
    for (name, version), reason in ALLOWED_YANKED.items():
        print(f"Allowed yanked {name} {version}: {reason}")
    for (id, name, version), reason in ALLOWED_ADVISORIES.items():
        print(f"Allowed {id} on {name} {version}: {reason}")
    found = problems(report)
    if found:
        sys.exit("cargo audit denies:\n" + "\n".join(f"- {line}" for line in found))
    print("cargo audit denies nothing.")


if __name__ == "__main__":
    main()
