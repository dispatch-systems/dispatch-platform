"""Real-process checks of the installed launch path: the launcher runs only an installed
manager and never the candidate it verifies."""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from runtime_fixtures import TOOLING, artifact, private_directory, write_json


class InstalledLauncherTests(unittest.TestCase):
    def test_a_missing_manager_is_an_error_and_the_candidate_is_never_executed(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "public"
            root.mkdir(mode=0o700)
            manifest = artifact(root / "live", "a" * 40)
            write_json(root / "data/platform/production-update.json",
                       {"status": "ready", "commit": "a" * 40, "digest": manifest["digest"]})
            management = private_directory(root / "management")
            for name in ["runtime_artifact.py", "update-production.py"]:
                shutil.copyfile(TOOLING / name, management / name)
            result = subprocess.run([sys.executable, str(management / "update-production.py"),
                                     "--root", str(root), "--verify"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Installed host management is missing", result.stderr)
            self.assertFalse((management / "dispatch-host").exists())
            self.assertFalse((root / "executed").exists())


if __name__ == "__main__":
    unittest.main()
