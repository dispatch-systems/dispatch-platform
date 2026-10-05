"""The host setup launchers, the collector shards and what the ship command knows of the gate."""
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


class PipelineTests(unittest.TestCase):
    def test_setup_launchers_use_trusted_host_and_preserve_paths_and_owner_names(self):
        args = ["--root", "/private path/dev", "--first-name", "Two Names"]
        for environment in ["dev", "production"]:
            launcher = module(f"setup_{environment}", f"ops/launchers/setup-{environment}.py")
            with patch.object(launcher, "host_binary", return_value=Path("/trusted path/dispatch-host")), \
                    patch.object(launcher.os, "execv") as execute:
                launcher.main(args)
                execute.assert_called_once_with("/trusted path/dispatch-host", [
                    "/trusted path/dispatch-host", "host", "setup", environment, *args])

    def test_collector_shards_preserve_coverage_and_isolate_capacity(self):
        files = [file for shard in collectors.SHARDS.values() for file in shard]
        self.assertEqual(len(files), len(set(files)))
        # Every suite in a shard exists; a new collector adds a shard of its own.
        self.assertEqual([file for file in files if not (ROOT / file).is_file()], [])
        self.assertLessEqual({
            "collectors/paycom/tests/native/paycom-worker.test.ts", "collectors/paycom/tests/native/native-browser.test.ts",
            "collectors/paycom/tests/native/native-browser-recovery.test.ts",
            "collectors/cortex/tests/native/cortex-worker.test.ts", "collectors/cortex/tests/native/cortex-meals-worker.test.ts",
            "collectors/cortex/tests/native/cortex-scorecard-worker.test.ts",
            "collectors/cortex/tests/native/cortex-routes-worker.test.ts",
            "features/timecard/tests/native/meal-sync-worker.test.ts",
            "app/tests/native/multi-dsp-browser.test.ts", "app/tests/native/collection-throughput.test.ts",
        }, set(files))
        self.assertEqual(set(collectors.SHARDS["capacity"]), {
            "app/tests/native/multi-dsp-browser.test.ts", "app/tests/native/collection-throughput.test.ts",
        })
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

    def test_ship_knows_every_job_the_gate_lets_fail(self):
        # dispatchdev ship stops at a queue run's first failed job unless the gate lets that job fail.
        workflow = (ROOT / ".github/workflows/checks.yml").read_text()
        jobs = mapping_fields(mapping_fields(workflow)["jobs"][1])
        advisory = {job for job, (_, body) in jobs.items()
                    if mapping_fields(body).get("continue-on-error", ("false", ""))[0] == "true"}
        listed = re.search(r"\bconst\s+ADVISORY\b[^=]*=\s*&\s*\[([^\]]*)\]",
                           (ROOT / "tooling/cli/src/ship.rs").read_text())
        self.assertIsNotNone(listed)
        self.assertEqual(set(re.findall(r'"([a-z-]+)"', listed.group(1))), advisory)
        self.assertTrue(advisory)


def mapping_fields(source):
    """The direct fields of a YAML block mapping, independent of indentation width."""
    lines = source.splitlines()
    significant = [(i, line) for i, line in enumerate(lines)
                   if line.strip() and not line.lstrip().startswith("#")]
    if not significant:
        return {}
    indent = min(len(line) - len(line.lstrip()) for _, line in significant)
    fields = [(i, line) for i, line in significant if len(line) - len(line.lstrip()) == indent]
    result = {}
    for at, (i, line) in enumerate(fields):
        match = re.fullmatch(r"\s*([a-zA-Z_-]+):\s*(.*)", line)
        if match:
            end = fields[at + 1][0] if at + 1 < len(fields) else len(lines)
            result[match[1]] = (match[2], "\n".join(lines[i + 1:end]))
    return result


if __name__ == "__main__":
    unittest.main()
