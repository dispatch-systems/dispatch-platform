"""Exercise actual assigned-workspace guards, rather than mirroring profile declarations."""
import importlib.util
import json
import os
import shutil
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("dispatch_support_plugin", Path(__file__).parents[2] / "services/support-worker/hermes/plugin/__init__.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.root = self.base / "assigned"
        self.root.mkdir()
        (self.root / ".git").write_text("gitdir: /outside/metadata")
        (self.root / "safe.txt").write_text("original")
        profiles = Path(__file__).parents[2] / "services/support-worker/hermes/profiles"
        self.profiles = {role: json.loads((profiles / f"{role}.json").read_text())
                         for role in ("analyst", "builder", "reviewer")}
        self.request = {"worktree":str(self.root), "role":"builder", "profile":self.profiles["builder"],
                        "node":os.environ.get("SUPPORT_TEST_NODE", shutil.which("node"))}

    def tearDown(self):
        self.temp.cleanup()

    def test_escape_symlink_private_and_git_admin_paths_never_reach_file_handlers(self):
        policy = module.Policy(self.request)
        (self.root / "escape").symlink_to(self.base)
        for path in ("../secret", "escape/secret", ".git", ".env", "private/token"):
            with self.assertRaises(PermissionError):
                policy.path(path)
        self.assertEqual(policy.path("safe.txt"), self.root / "safe.txt")

    def test_analyst_and_reviewer_cannot_edit_even_valid_assigned_files(self):
        for role in ("analyst", "reviewer"):
            policy = module.Policy({**self.request, "role":role, "profile":self.profiles[role]})
            with self.assertRaises(PermissionError):
                policy.write({"path":"safe.txt", "content":"changed"})
            with self.assertRaises(PermissionError):
                policy.patch({"path":"safe.txt", "old_string":"original", "new_string":"changed"})
        self.assertEqual((self.root / "safe.txt").read_text(), "original")

    def test_intake_needs_no_workspace_and_source_analysis_cannot_open_a_browser(self):
        analyst = self.profiles["analyst"]
        intake = {**analyst, **analyst["stages"]["intake"]}
        request = {**self.request, "role":"analyst", "profile":intake,
                   "worktree":None, "home":str(self.base), "context":{}}
        policy = module.Policy(request)
        for handler, arguments in ((policy.read, {"path":"safe.txt"}),
                                   (policy.command, {"command":"printf unexpected"}),
                                   (policy.diff, {}),
                                   (policy.browse, {"action":"snapshot"})):
            with self.assertRaises(PermissionError):
                handler(arguments)
        source = {**analyst, **analyst["stages"]["source"]}
        policy = module.Policy({**request, "profile":source, "worktree":str(self.root)})
        with self.assertRaises(PermissionError):
            policy.browse({"action":"snapshot"})
        self.assertFalse((self.base / "browser-attempt.json").exists())
        failed = json.loads(policy.command({"command":"exit 17"}))
        self.assertEqual(failed["exitCode"], 17)
        self.assertEqual(policy.observations, [])
        observed = json.loads(policy.command({"command":"cat safe.txt"}))
        self.assertEqual(observed["exitCode"], 0)
        self.assertEqual(observed["stdout"], "original")
        self.assertEqual(policy.observations, ["dispatch_command"])
        self.assertEqual((self.root / "safe.txt").read_text(), "original")

    def test_shell_can_edit_only_builder_source_and_cannot_replace_host_git_pointer(self):
        builder = module.Policy(self.request)
        result = json.loads(builder.command({"command":"test \"$HOME\" = /sandbox/home && test -d \"$HOME\" && printf changed > safe.txt"}))
        self.assertEqual(result["exitCode"], 0, result["stderr"])
        self.assertEqual((self.root / "safe.txt").read_text(), "changed")
        result = json.loads(builder.command({"command":"printf escaped > .git"}))
        self.assertNotEqual(result["exitCode"], 0)
        self.assertEqual((self.root / ".git").read_text(), "gitdir: /outside/metadata")
        reviewer = module.Policy({**self.request, "role":"reviewer", "profile":self.profiles["reviewer"]})
        result = json.loads(reviewer.command({"command":"printf unauthorized > safe.txt"}))
        self.assertNotEqual(result["exitCode"], 0)
        self.assertEqual((self.root / "safe.txt").read_text(), "changed")


if __name__ == "__main__":
    unittest.main()
