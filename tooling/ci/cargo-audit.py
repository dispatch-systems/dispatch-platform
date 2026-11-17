#!/usr/bin/env python3
"""`cargo audit --deny warnings`, less the yanked crate versions and advisories listed here:
`cargo-audit.py` runs the audit and fails on any vulnerability or warning, a yanked crate
included, unless that exact version, or that advisory on that exact version, a vulnerability's
or a warning's, is allowed below with why it stays. An allowed entry the audit no longer finds
fails it too, so each entry leaves with the dependency that needed it."""
import json
import subprocess
import sys

# Yanked versions the lockfile may hold, each with why it is safe and when it goes.
ALLOWED_YANKED = {}

# Advisories the lockfile may hold, each on one exact version, with why it is safe and when it
# goes.
WEBAUTHN_RSA = (
    "rsa has no fixed release. The Marvin attack recovers an RSA private key from the timing of "
    "decryption with it; webauthn-rs, the only user of rsa here, through crypto-glue, only "
    "verifies authenticators' RS256 signatures with their public keys, and Dispatch holds no RSA "
    "private key. Goes once rsa releases a fix or webauthn-rs moves off it."
)
CHRONO = (
    "chrono and chrono-tz are declared unmaintained, with no vulnerability named and no release "
    "to move to. Dispatch keeps every time and timezone with them, across its crates, so leaving "
    "them is a migration of its own to jiff or time. Goes once Dispatch moves off chrono, or a "
    "maintained release supersedes these."
)
ALLOWED_ADVISORIES = {
    ("RUSTSEC-2023-0071", "rsa", "0.10.0-rc.19"): WEBAUTHN_RSA,
    ("RUSTSEC-2026-0543", "chrono", "0.4.45"): CHRONO,
    ("RUSTSEC-2026-0555", "chrono-tz", "0.10.4"): CHRONO,
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
            if (advisory, *package) in advisories:
                seen_advisories.add((advisory, *package))
                continue
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
