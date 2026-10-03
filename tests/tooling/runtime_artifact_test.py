import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import unittest.mock
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[2] / "tooling"))
import runtime_artifact as runtime


class SharedToolingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dispatch-runtime-artifact-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_host_passes_on_what_it_reports_beside_a_success(self):
        process = unittest.mock.Mock(returncode=0)
        process.communicate.return_value = ("null", "Installed the new build\n")
        with patch.object(runtime, "host_binary", return_value=Path("/tmp/host")), \
                patch.object(runtime.subprocess, "Popen", return_value=process), \
                patch.object(runtime.sys, "stderr", new_callable=io.StringIO) as stderr:
            self.assertIsNone(runtime.host("dev", "--root", self.root))
        self.assertEqual(stderr.getvalue(), "Installed the new build\n")

    def test_source_verifier_uses_cargos_configured_output_and_never_a_stale_default(self):
        (self.root / "ops/host-manager").mkdir(parents=True)
        (self.root / "ops/host-manager/Cargo.toml").touch()
        (self.root / "tooling").mkdir()
        custom = self.root / "custom-target"
        with patch.object(runtime, "__file__", str(self.root / "tooling/runtime_artifact.py")), \
                patch.object(runtime.subprocess, "check_call") as build, \
                patch.object(runtime, "command", return_value=json.dumps({"target_directory": str(custom)})):
            self.assertEqual(runtime.host_binary.__wrapped__(), custom / "release/dispatch-host")
            build.assert_called_once_with(["cargo", "build", "--locked", "--release", "--workspace", "--bin", "dispatch-host"],
                                          cwd=self.root, stdout=sys.stderr)

    def test_source_verifier_prefers_a_restored_host_only_from_its_own_ci_cache(self):
        (self.root / "ops/host-manager").mkdir(parents=True)
        (self.root / "ops/host-manager/Cargo.toml").touch()
        (self.root / "tooling").mkdir()
        tools = self.root / ".ci-tools"
        binary = tools / "tools/dispatch-host"
        binary.parent.mkdir(parents=True)
        binary.write_text("#!/bin/sh\n")
        binary.chmod(0o700)
        trusted = {"CI": "true", "DISPATCH_CI_TOOLS": str(tools)}
        with patch.object(runtime, "__file__", str(self.root / "tooling/runtime_artifact.py")), \
                patch.object(runtime.subprocess, "check_call") as build, \
                patch.object(runtime, "command", return_value=json.dumps({"target_directory": str(self.root / "target")})):
            with patch.dict(os.environ, trusted, clear=False):
                self.assertEqual(runtime.host_binary.__wrapped__(), binary)
                build.assert_not_called()
                binary.chmod(0o600)
                self.assertEqual(runtime.host_binary.__wrapped__(), self.root / "target/release/dispatch-host",
                                 "a non-executable file is not a tool")
                binary.chmod(0o700)
            for untrusted in [{"DISPATCH_CI_TOOLS": str(tools)}, {"CI": "true"},
                              {"CI": "true", "DISPATCH_CI_TOOLS": str(self.root / "elsewhere")}]:
                with patch.dict(os.environ, untrusted, clear=True):
                    self.assertEqual(runtime.host_binary.__wrapped__(), self.root / "target/release/dispatch-host",
                                     untrusted)
            self.assertEqual(build.call_count, 4)


if __name__ == "__main__":
    unittest.main()
