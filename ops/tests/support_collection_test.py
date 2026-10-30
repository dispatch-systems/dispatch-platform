import importlib.util
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest
import unittest.mock

spec = importlib.util.spec_from_file_location(
    "support_collection", Path(__file__).resolve().parents[2] / "ops/launchers/support-collection.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SupportCollectionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "dev"
        self.dsp = "dsp_" + "a" * 32
        platform = self.source / "data/platform"
        platform.mkdir(parents=True)
        with sqlite3.connect(platform / "accounts.sqlite") as db:
            db.execute("CREATE TABLE dsps(id TEXT,status TEXT,name TEXT,timezone TEXT)")
            db.execute("INSERT INTO dsps VALUES(?, 'active', 'private account name', 'America/Chicago')", (self.dsp,))
            db.execute("INSERT INTO dsps VALUES(?, 'active', 'unrelated account sentinel', 'UTC')", ("dsp_" + "b" * 32,))
        area = self.source / "dsps" / self.dsp
        area.joinpath("data").mkdir(parents=True)
        with sqlite3.connect(area / "data/dispatch.sqlite") as db:
            db.execute("CREATE TABLE settings(key TEXT,value TEXT)")
            db.execute("INSERT INTO settings VALUES('dsp.profile', \
                '{\"abbreviation\":\"TEST\",\"stationCode\":\"TST1\",\"unrelated\":\"secret sentinel\"}')")
        for provider in ("paycom", "cortex"):
            path = area / "data" / provider
            path.mkdir(parents=True)
            with sqlite3.connect(path / f"{provider}.sqlite") as db:
                db.execute("CREATE TABLE connections(provider TEXT,enabled INTEGER,revision INTEGER)")
                db.execute("INSERT INTO connections VALUES(?,1,7)", (provider,))
                db.execute("CREATE TABLE employee_private_data(name TEXT)")
                db.execute("INSERT INTO employee_private_data VALUES('employee sentinel')")
        secrets = area / "secrets"
        secrets.mkdir()
        for name in ("vault.key", "paycom.enc", "cortex.enc"):
            (secrets / name).write_text("opaque fixture " + name)
        for path in self.source.rglob("*"):
            if path.is_file():
                path.chmod(0o600)

    def test_an_assigned_worktree_is_found_through_a_linked_worktrees_folder(self):
        # worktrees/ links to a disk of its own, where the worktree really is.
        elsewhere = self.root / "elsewhere"
        ticket = "support_" + "c" * 32
        worktree = elsewhere / ("support-dev-" + "c" * 32)
        worktree.mkdir(parents=True)
        (self.root / "worktrees").symlink_to(elsewhere)
        argv = ["support-collection.py", "--worktree", str(self.root / "worktrees" / worktree.name),
                "--ticket", ticket, "--provider", "paycom", "--request", "{}"]
        with unittest.mock.patch.dict(os.environ, {"SUPPORT_WORKTREES": str(self.root / "worktrees")}), \
                unittest.mock.patch("sys.argv", argv), \
                unittest.mock.patch.object(module.subprocess, "check_output", return_value="elsewhere\n"):
            # Past the worktree's check, to its branch, which isn't the ticket's.
            with self.assertRaisesRegex(ValueError, "Assigned support branch required"):
                module.main()

    def test_selected_projection_excludes_other_accounts_providers_and_record_tables(self):
        broker = self.root / "broker/dev"
        self.assertEqual(module.project_account(self.source, self.dsp, "paycom", broker), (1, 7))
        self.assertFalse((broker / "dsps" / self.dsp / "secrets/cortex.enc").exists())
        self.assertFalse((broker / "dsps" / self.dsp / "data/cortex").exists())
        with sqlite3.connect(broker / "data/platform/accounts.sqlite") as db:
            self.assertEqual(db.execute("PRAGMA table_info(dsps)").fetchall()[0][1], "id")
            self.assertEqual([column[1] for column in db.execute("PRAGMA table_info(dsps)")], ["id", "status", "name", "timezone"])
            self.assertEqual(db.execute("SELECT id,name FROM dsps").fetchall(), [(self.dsp, "private account name")])
        with sqlite3.connect(broker / "dsps" / self.dsp / "data/dispatch.sqlite") as db:
            projected = db.execute("SELECT value FROM settings").fetchone()[0]
            self.assertIn("TST1", projected)
            self.assertNotIn("secret sentinel", projected)
        with sqlite3.connect(broker / "dsps" / self.dsp / "data/paycom/paycom.sqlite") as db:
            self.assertEqual(db.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall(), [("connections",)])
        self.assertEqual((broker / "dsps" / self.dsp / "secrets/paycom.enc").read_text(), "opaque fixture paycom.enc")

    def test_disabled_account_or_linked_secret_never_grants_a_collector(self):
        path = self.source / "dsps" / self.dsp / "data/paycom/paycom.sqlite"
        with sqlite3.connect(path) as db:
            db.execute("UPDATE connections SET enabled=0")
        with self.assertRaises(ValueError):
            module.project_account(self.source, self.dsp, "paycom", self.root / "disabled")
        self.assertFalse((self.root / "disabled").exists())
        with sqlite3.connect(path) as db:
            db.execute("UPDATE connections SET enabled=1")
        secret = self.source / "dsps" / self.dsp / "secrets/paycom.enc"
        os.link(secret, self.root / "hardlink.enc")
        with self.assertRaises(ValueError):
            module.project_account(self.source, self.dsp, "paycom", self.root / "linked")

    def test_branch_state_cannot_redirect_trusted_database_reads_through_an_ancestor(self):
        state = self.root / "state"
        data = state / "data"
        data.mkdir(parents=True, mode=0o700)
        (data / "platform").symlink_to(self.source / "data/platform")
        with self.assertRaises(ValueError):
            module.private_state_file(state, Path("data/platform/accounts.sqlite"))


if __name__ == "__main__":
    unittest.main()
