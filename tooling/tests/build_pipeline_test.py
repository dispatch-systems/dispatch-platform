"""The host setup launchers and the collector shards."""
import importlib.util
import io
from pathlib import Path
import re
import sys
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).parents[2]
sys.path.insert(0, str(ROOT / "tooling/ci"))
sys.path.insert(0, str(ROOT / "ops/launchers"))


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


collectors = module("browseros_check", "tooling/ci/browseros-check.py")
audit = module("cargo_audit", "tooling/ci/cargo-audit.py")


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


class PipelineTests(unittest.TestCase):
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

    def test_setup_launchers_use_trusted_host_and_preserve_paths_and_owner_names(self):
        args = ["--root", "/private path/dev", "--first-name", "Two Names"]
        for environment in ["dev", "production"]:
            launcher = module(f"setup_{environment}", f"ops/launchers/setup-{environment}.py")
            with patch.object(launcher, "host_binary", return_value=Path("/trusted path/dispatch-host")), \
                    patch.object(launcher.os, "execv") as execute:
                launcher.main(args)
                execute.assert_called_once_with("/trusted path/dispatch-host", [
                    "/trusted path/dispatch-host", "host", "setup", environment, *args])

    def test_collector_shards_isolate_capacity_and_require_the_real_timeout(self):
        # The app's multi-DSP suites run on the capacity runner, and nothing else does.
        self.assertEqual(set(collectors.SHARDS["capacity"]),
                         {path.relative_to(ROOT).as_posix() for path in (ROOT / "app/tests/native").glob("*.test.ts")})
        title = collectors.REAL_TIMEOUT["title"]
        passed = f'<testsuites><testcase name="{title}" /></testsuites>'
        collectors.require_real_timeout(passed)
        for report in ["invalid", "<testsuites />", passed.replace(" />", "><skipped /></testcase>"),
                       passed.replace(" />", "><failure /></testcase>"),
                       passed.replace(" />", "><error /></testcase>"),
                       f'<testsuites><testcase name="{title}" /><testcase name="{title}" /></testsuites>']:
            with self.subTest(report=report), self.assertRaises(SystemExit):
                collectors.require_real_timeout(report)
        for real in [False, True]:
            args = ["browseros-check.py", "--real-timeouts"] if real else ["browseros-check.py", "--shard", "paycom-recovery"]
            done = subprocess.CompletedProcess([], 0, passed, "")
            with patch.object(sys, "argv", args), patch.object(collectors.subprocess, "run", return_value=done) as run, \
                    patch.dict(collectors.os.environ, {"DISPATCH_TEST_REAL_TIMEOUTS": "1"}), \
                    patch.object(sys, "stdout", new_callable=io.StringIO):
                collectors.main()
            self.assertEqual(run.call_count, 2, "build and selected Node tests only; no live host probe")
            command = run.call_args.args[0]
            env = run.call_args.kwargs["env"]
            self.assertEqual(env["DISPATCH_TEST_NATIVE"], "1")
            if real:
                self.assertEqual(env["DISPATCH_TEST_REAL_TIMEOUTS"], "1")
                self.assertIn("--test-reporter=junit", command)
                self.assertIn("--test-name-pattern=^" + re.escape(title) + "$", command)
                self.assertEqual(command[-1], collectors.REAL_TIMEOUT["file"])
            else:
                self.assertNotIn("DISPATCH_TEST_REAL_TIMEOUTS", env)
                self.assertNotIn("--test-reporter=junit", command)


if __name__ == "__main__":
    unittest.main()
