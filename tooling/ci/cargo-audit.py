#!/usr/bin/env python3
"""`cargo audit --deny warnings`, less the yanked crate versions listed here: `cargo-audit.py`
runs the audit and fails on any vulnerability or warning, a yanked crate included, unless that
exact version is allowed below with why it stays. An allowed version the audit no longer finds
fails it too, so each entry leaves with the dependency that needed it."""
import json
import subprocess
import sys

# Yanked versions the lockfile may hold, each with why it is safe and when it goes.
ALLOWED_YANKED = {
    ("zip", "8.6.0"): (
        "calamine 0.36, which reads Cortex's .xlsx exports, requires zip ^8.6, and every zip "
        "before 9.0.3 was yanked after advisories about extracting archives to disk, which "
        "calamine never does. Remove it once calamine moves to zip 9."
    ),
}


def problems(report, allowed=ALLOWED_YANKED):
    """What fails the audit, from cargo audit's JSON `report`."""
    found = [
        f"{entry['advisory']['id']}: {entry['package']['name']} {entry['package']['version']}"
        for entry in report["vulnerabilities"]["list"]
    ]
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
    found = problems(report)
    if found:
        sys.exit("cargo audit denies:\n" + "\n".join(f"- {line}" for line in found))
    print("cargo audit denies nothing.")


if __name__ == "__main__":
    main()
