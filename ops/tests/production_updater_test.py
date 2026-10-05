"""Installed Production launcher compatibility; updater policy lives in Rust tests."""
import subprocess
import sys
import unittest
from dev_updater_test import TOOLING


class ProductionLauncherTests(unittest.TestCase):

    def test_legacy_systemd_paths_remain_callable_and_use_no_github_credentials(self):
        for environment in ["dev", "production"]:
            launcher = TOOLING / f"update-{environment}.py"
            result = subprocess.run([sys.executable, str(launcher), "--help"], capture_output=True)
            self.assertEqual(result.returncode, 0)
            self.assertIn(b"--verify", result.stdout)
            unit = (TOOLING.parent / f"systemd/dispatch-{environment}.service").read_text()
            self.assertIn(f"update-{environment}.py", unit)
            self.assertNotIn("--verify-management", unit)


if __name__ == "__main__":
    unittest.main()
