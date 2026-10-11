"""Verify changed commands cannot inherit host credentials or networking."""
import importlib.util
import json
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
    def test_collection_child_starts_with_its_canonical_pin_mounted_read_only(self):
        launchers = Path(__file__).resolve().parents[2] / "ops/launchers"
        pin = json.loads((launchers / "browseros-release.json").read_text())
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            artifact = base / "artifact"
            binary = artifact / "services/rust/dispatch-backend"
            binary.parent.mkdir(parents=True)
            binary.write_text("""#!/usr/bin/python3
import errno, json, os, pathlib, sys
assert sys.argv[1:] == ['support-verify-collection']
assert json.load(sys.stdin) == {'request': 'synthetic'}
pin = pathlib.Path('/runtime/browseros-release.json')
release = json.loads(pin.read_text())
assert os.statvfs(pin).f_flag & os.ST_RDONLY
try:
    descriptor = os.open(pin, os.O_WRONLY)
except OSError as error:
    assert error.errno == errno.EROFS
else:
    os.close(descriptor)
    raise AssertionError('canonical pin is writable')
print(json.dumps({'argv': sys.argv[1:], 'pin': release, 'readOnly': True}))
""")
            binary.chmod(0o700)
            state = base / "state"
            target = "dsp_" + "a" * 32
            (state / "dsps" / target / "secrets").mkdir(parents=True)
            for name in ("broker", "egress", "browser"):
                (base / name).mkdir()
            # Only path resolution is needed here; the real wrapper's child will execute
            # the witness backend, never BrowserOS or a provider collector.
            (base / "browser/browseros").write_text("synthetic browser install")
            browser = f"/opt/dispatch-browseros/{pin['version']}"
            sandbox = module.trusted_sandbox()
            outer = [str(sandbox), "--unshare-all", "--die-with-parent", "--new-session", "--cap-drop", "ALL",
                     "--clearenv", "--ro-bind", "/usr", "/usr", "--symlink", "usr/bin", "/bin",
                     "--symlink", "usr/lib", "/lib", "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]
            if Path("/usr/lib64").exists():
                outer.extend(["--symlink", "usr/lib64", "/lib64"])
            for source, destination, writable in [
                (artifact, "/artifact", False),
                (state, "/state", True),
                (base / "broker", "/broker", True),
                (base / "egress", "/egress", False),
                (base / "browser", browser, False),
                (launchers / "support-collection.py", "/collection/support-collection.py", False),
                (launchers / "browseros-release.json", "/collection/browseros-release.json", False),
            ]:
                outer.extend(["--bind" if writable else "--ro-bind", str(source), destination])
            launcher = """import importlib.util, json, pathlib, subprocess, sys
spec = importlib.util.spec_from_file_location('collection', '/collection/support-collection.py')
collection = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collection)
command = collection.sandbox(pathlib.Path('/artifact'), pathlib.Path('/state'),
    {'DISPATCH_BROWSEROS_EXECUTABLE': str(collection.BROWSER), 'DISPATCH_BWRAP_EXECUTABLE': sys.argv[1]},
    pathlib.Path('/broker'), pathlib.Path('/egress'), 'dsp_' + 'a' * 32)
command.extend(['/usr/bin/python3', '/runtime/support-collection.py', '--sandbox-child',
    '/artifact/services/rust/dispatch-backend'])
result = subprocess.run(command, input=json.dumps({'request': 'synthetic'}), text=True)
sys.exit(result.returncode)
"""
            outer.extend(["/usr/bin/python3", "-I", "-B", "-c", launcher, str(sandbox)])
            result = subprocess.run(outer, capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            witness = json.loads(result.stdout)
            self.assertEqual(witness["argv"], ["support-verify-collection"])
            self.assertEqual(witness["pin"], pin)
            self.assertTrue(witness["readOnly"])

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
test "$HOME" = /sandbox/home
test -d "$HOME"
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
