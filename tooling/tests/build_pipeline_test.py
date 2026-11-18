"""The dependency audit's allowances, the issue a failing nightly audit opens, and the collector
shards."""
import importlib.util
from pathlib import Path
import re
import sys
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).parents[2]
sys.path.insert(0, str(ROOT / "tooling/ci"))


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


collectors = module("browseros_check", "tooling/ci/browseros-check.py")
audit = module("cargo_audit", "tooling/ci/cargo-audit.py")
npm_audit = module("npm_audit", "tooling/ci/npm-audit.py")
advisory_issue = module("advisory_issue", "tooling/ci/advisory-issue.py")


def audit_report(*warnings, vulnerabilities=()):
    """cargo audit's JSON report with `(kind, crate, version)` warnings and vulnerabilities."""
    grouped = {}
    for kind, name, version in warnings:
        advisory = None if kind == "yanked" else {"id": "RUSTSEC-2026-0001"}
        grouped.setdefault(kind, []).append(
            {"kind": kind, "package": {"name": name, "version": version}, "advisory": advisory}
        )
    listed = [
        {"advisory": {"id": id}, "package": {"name": name, "version": version}}
        for id, name, version in vulnerabilities
    ]
    return {"vulnerabilities": {"found": bool(listed), "count": len(listed), "list": listed},
            "warnings": grouped}


def npm_report(*advisories, dependents=()):
    """npm audit's JSON report with `(id, package, severity)` advisories, each installed at
    node_modules/<package>, and the `dependents` named only for depending on one."""
    listed = {
        name: {"via": [{"name": name, "severity": severity,
                        "url": f"https://github.com/advisories/{id}"}],
               "nodes": [f"node_modules/{name}"]}
        for id, name, severity in advisories
    }
    for name, on in dependents:
        listed[name] = {"via": [on], "nodes": [f"node_modules/{name}"]}
    return {"vulnerabilities": listed}


class PipelineTests(unittest.TestCase):
    def test_the_npm_audit_allows_only_the_listed_advisories_on_their_versions_and_drops_stale_ones(self):
        advisories = {("GHSA-aaaa", "glob", "4.0.7"): "why"}
        versions = {"node_modules/glob": "4.0.7", "node_modules/build": "8.0.0",
                    "node_modules/other": "1.0.0"}
        listed = ("GHSA-aaaa", "glob", "high")
        # What only depends on the allowed package adds nothing of its own.
        report = npm_report(listed, dependents=[("build", "glob")])
        self.assertEqual(npm_audit.problems(report, versions, advisories), [])
        # The same advisory on another version, another advisory on that version and one on
        # another package still fail it; one below high does not.
        self.assertEqual(
            npm_audit.problems(npm_report(listed), {**versions, "node_modules/glob": "4.0.8"}, advisories),
            ["GHSA-aaaa: glob 4.0.8",
             "GHSA-aaaa on glob 4.0.7 is allowed but no longer found: remove it from ALLOWED_ADVISORIES"],
        )
        report = npm_report(listed)
        report["vulnerabilities"]["glob"]["via"].append(
            {"name": "glob", "severity": "critical", "url": "https://github.com/advisories/GHSA-bbbb"})
        self.assertEqual(npm_audit.problems(report, versions, advisories), ["GHSA-bbbb: glob 4.0.7"])
        self.assertEqual(
            npm_audit.problems(npm_report(listed, ("GHSA-cccc", "other", "high")), versions, advisories),
            ["GHSA-cccc: other 1.0.0"],
        )
        self.assertEqual(
            npm_audit.problems(npm_report(listed, ("GHSA-cccc", "other", "moderate")), versions, advisories),
            [],
        )
        # Once the lockfile moves on, its entry has to go with it.
        self.assertEqual(
            npm_audit.problems(npm_report(), versions, advisories),
            ["GHSA-aaaa on glob 4.0.7 is allowed but no longer found: remove it from ALLOWED_ADVISORIES"],
        )

    def test_the_audit_allows_only_the_listed_yanked_versions_and_drops_stale_ones(self):
        allowed = {("zip", "8.6.0"): "why"}
        self.assertEqual(audit.problems(audit_report(("yanked", "zip", "8.6.0")), allowed, {}), [])
        # Another yanked version, any other warning and any vulnerability still fail it.
        for report in [
            audit_report(("yanked", "zip", "8.6.0"), ("yanked", "flate2", "1.0.0")),
            audit_report(("yanked", "zip", "8.6.0"), ("unmaintained", "zip", "8.6.0")),
            audit_report(("yanked", "zip", "8.6.0"), vulnerabilities=[("RUSTSEC-2026-0002", "zip", "8.6.0")]),
        ]:
            self.assertEqual(len(audit.problems(report, allowed, {})), 1, report)
        # Once the lockfile moves on, its entry has to go with it.
        self.assertEqual(
            audit.problems(audit_report(), allowed, {}),
            ["zip 8.6.0 is allowed yanked but no longer found: remove it from ALLOWED_YANKED"],
        )

    def test_the_audit_allows_only_the_listed_advisories_on_their_versions_and_drops_stale_ones(self):
        advisories = {("RUSTSEC-2026-0367", "zip", "8.6.0"): "why"}
        listed = ("RUSTSEC-2026-0367", "zip", "8.6.0")
        self.assertEqual(audit.problems(audit_report(vulnerabilities=[listed]), {}, advisories), [])
        # A warning's advisory is allowed the same way, as an unmaintained crate's.
        warned = {("RUSTSEC-2026-0001", "zip", "8.6.0"): "why"}
        self.assertEqual(audit.problems(audit_report(("unmaintained", "zip", "8.6.0")), {}, warned), [])
        self.assertEqual(len(audit.problems(audit_report(("unmaintained", "zip", "9.0.4")), {}, warned)), 2)
        # The same advisory on another version, another advisory on that version, and a
        # warning still fail it.
        for report in [
            audit_report(vulnerabilities=[listed, ("RUSTSEC-2026-0367", "zip", "9.0.4")]),
            audit_report(vulnerabilities=[listed, ("RUSTSEC-2026-0002", "zip", "8.6.0")]),
            audit_report(("unmaintained", "zip", "8.6.0"), vulnerabilities=[listed]),
        ]:
            self.assertEqual(len(audit.problems(report, {}, advisories)), 1, report)
        # Once the lockfile moves on, its entry has to go with it.
        self.assertEqual(
            audit.problems(audit_report(), {}, advisories),
            ["RUSTSEC-2026-0367 on zip 8.6.0 is allowed but no longer found: remove it from "
             "ALLOWED_ADVISORIES"],
        )

    def test_a_failing_nightly_audit_keeps_one_issue_open_until_a_nightly_passes(self):
        run = "https://github.com/o/r/actions/runs/1"
        denied = ["cargo audit denies:", "- RUSTSEC-2026-0367: zip 8.6.0"]
        body = f"{run}\n\ncargo audit denies:\n- RUSTSEC-2026-0367: zip 8.6.0"
        # The first failure opens the issue, naming the run and what the audit denied.
        self.assertEqual(advisory_issue.commands("failure", [], denied, run), [
            ["create", "--title", advisory_issue.TITLE, "--label", "main-advisory", "--body", body],
        ])
        # Another while one is open comments on it, the oldest if someone opened a second.
        self.assertEqual(advisory_issue.commands("failure", [12, 7], denied, run),
                         [["comment", "7", "--body", body]])
        # An audit that gave no report still says so.
        self.assertEqual(advisory_issue.commands("failure", [7], [], run),
                         [["comment", "7", "--body", f"{run}\n\nThe audit gave no report: see the run."]])
        # A pass closes every one open, and with none open does nothing.
        self.assertEqual(advisory_issue.commands("success", [12, 7], [], run), [
            ["close", "7", "--comment", f"Passes again: {run}"],
            ["close", "12", "--comment", f"Passes again: {run}"],
        ])
        self.assertEqual(advisory_issue.commands("success", [], [], run), [])
        # A cancelled audit proves nothing either way.
        self.assertEqual(advisory_issue.commands("cancelled", [7], denied, run), [])

    def test_collector_shards_isolate_capacity(self):
        # The app's multi-DSP suites run on the capacity runner, and nothing else does.
        self.assertEqual(set(collectors.SHARDS["capacity"]),
                         {path.relative_to(ROOT).as_posix() for path in (ROOT / "app/tests/native").glob("*.test.ts")})
        done = subprocess.CompletedProcess([], 0, "", "")
        with patch.object(sys, "argv", ["browseros-check.py", "--shard", "paycom-recovery"]), \
                patch.object(collectors.subprocess, "run", return_value=done) as run:
            collectors.main()
        self.assertEqual(run.call_count, 2, "build and selected Node tests only; no live host probe")
        self.assertEqual(run.call_args.kwargs["env"]["DISPATCH_TEST_NATIVE"], "1")
        self.assertEqual(run.call_args.args[0][-1], collectors.SHARDS["paycom-recovery"][-1])
        # The host shard runs the live host probe and no Node tests.
        with patch.object(sys, "argv", ["browseros-check.py", "--shard", "host"]), \
                patch.object(collectors.subprocess, "run", return_value=done) as run:
            collectors.main()
        self.assertEqual([call.args[0][:2] for call in run.call_args_list],
                         [["tooling/cli/dispatchdev", "build"], ["cargo", "test"]])


if __name__ == "__main__":
    unittest.main()
