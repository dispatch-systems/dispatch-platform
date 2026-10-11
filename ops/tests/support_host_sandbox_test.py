"""Verify changed commands cannot inherit host credentials or networking."""
import importlib.util
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("support_host_sandbox", Path(__file__).parents[2] / "services/support-worker/hermes/host-sandbox.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SandboxTests(unittest.TestCase):
    def test_changed_command_has_only_assigned_source_projected_git_and_no_host_network(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            dev = base / "dev"
            root = base / "worktrees/support-sandbox-proof"
            subprocess.run(["git", "init", "-q", str(dev)], check=True)
            subprocess.run(["git", "-C", str(dev), "config", "user.email", "sandbox@dispatch.test"], check=True)
            subprocess.run(["git", "-C", str(dev), "config", "user.name", "Sandbox"], check=True)
            (dev / "source.txt").write_text("source")
            subprocess.run(["git", "-C", str(dev), "add", "."], check=True)
            subprocess.run(["git", "-C", str(dev), "commit", "-qm", "initial"], check=True)
            subprocess.run(["git", "-C", str(dev), "update-ref", "refs/remotes/origin/main", "HEAD"], check=True)
            subprocess.run(["git", "-C", str(dev), "worktree", "add", "-qb", root.name, str(root)], check=True)
            secret = base / "host-secret"
            secret.write_text("must never enter changed code")
            host_config = (dev / ".git/config").read_bytes()
            node = Path(shutil.which("node"))
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                listener.listen()
                port = listener.getsockname()[1]
                command = f"""set -e
test ! -e '{secret}'
test -z "${{SUPPORT_SANDBOX_TEST_SECRET:-}}"
git rev-parse HEAD
git config private.changed isolated
printf changed > source.txt
python3 -c 'import socket; sock=socket.socket(); sock.settimeout(1); assert sock.connect_ex(("127.0.0.1", {port})) != 0'
"""
                old = os.environ.get("SUPPORT_SANDBOX_TEST_SECRET")
                os.environ["SUPPORT_SANDBOX_TEST_SECRET"] = "must-not-inherit"
                try:
                    self.assertEqual(module.build(root, node, command), 0)
                finally:
                    if old is None:
                        os.environ.pop("SUPPORT_SANDBOX_TEST_SECRET", None)
                    else:
                        os.environ["SUPPORT_SANDBOX_TEST_SECRET"] = old
            self.assertEqual((root / "source.txt").read_text(), "changed")
            self.assertEqual((dev / ".git/config").read_bytes(), host_config)
            self.assertEqual(secret.read_text(), "must never enter changed code")
            shutil.rmtree(Path(f"/tmp/dispatch-{root.name}"), ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
