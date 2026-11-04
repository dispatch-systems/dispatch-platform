import contextlib
from datetime import datetime, timezone
import importlib.util
import io
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("caches_ci", ROOT / "tooling/ci/caches.py")
caches = importlib.util.module_from_spec(spec)
spec.loader.exec_module(caches)
Cache = caches.Cache


# A day after the last of them: every entry below was last read more than a day ago.
NOW = datetime(2026, 11, 20, tzinfo=timezone.utc)


def entries(key: str, *days: int, ref: str = "refs/heads/main", read: int = 0) -> list:
    """`key`'s entries on `ref` saved on each of `days`, `{}` in `key` taking the day, each last
    read when saved or on day `read`."""
    return [
        Cache(day, key.format(day), ref, f"2026-11-{day:02d}T00:00:00Z",
              f"2026-11-{max(day, read):02d}T00:00:00Z")
        for day in days
    ]


class PruneTest(unittest.TestCase):
    def test_deletes_only_what_newer_ones_of_the_same_kind_replaced(self):
        listed = [
            *entries("v0-rust-2026-W45-dispatch-release-Linux-x64-019b21e0-0000000{}", 1, 4, 2, 3),
            # Debug's entries are another kind: release's newer ones replace none of them.
            *entries("v0-rust-2026-W45-dispatch-debug-Linux-x64-019b21e0-0000000{}", 5, 6),
            *entries("v0-rust-2026-W46-dispatch-debug-Linux-x64-019b21e0-0000000{}", 7),
            *entries("dispatch-playwright-Linux-X64-1.6{}.0", 1, 2, 3),
            *entries("dispatch-node-modules-Linux-X64-{}aa", 1, 2, 3),
            *entries("dispatch-rust-toolchain-Linux-X64-{}ff", 1, 2, 3),
            *entries(
                "codeql-overlay-base-database-1-58b1deea8a9e87c1-javascript-2.27.4-"
                f"{'b' * 40}-4177{{}}-1",
                1, 2, 3,
            ),
            # The tools keep ten, each kind and image apart.
            *entries("dispatch-tools-Linux-X64-ubuntu24-{:02d}ee", *range(1, 12)),
            *entries("dispatch-fixture-Linux-X64-ubuntu24-{:02d}ee", 1, 2),
            *entries("dispatch-rust-binary-debug-Linux-X64-{}dd", 1, 2, 3, 4, 5),
            *entries("dispatch-rust-binary-release-Linux-X64-{}dd", 1, 2, 3, 4, 5),
            # No kind: binaries keyed before their profile was, Gradle's own, and whatever comes
            # next.
            *entries("dispatch-rust-binary-Linux-X64-{}bb", 1, 2, 3),
            *entries("gradle-transforms-v2-{}cc", 1, 2, 3),
        ]
        deleted = [cache.key for cache in caches.superseded(listed, NOW)]
        self.assertEqual(deleted, [
            "v0-rust-2026-W45-dispatch-release-Linux-x64-019b21e0-00000001",
            "dispatch-playwright-Linux-X64-1.61.0",
            "dispatch-node-modules-Linux-X64-1aa",
            "dispatch-rust-toolchain-Linux-X64-1ff",
            f"codeql-overlay-base-database-1-58b1deea8a9e87c1-javascript-2.27.4-{'b' * 40}-41771-1",
            "dispatch-tools-Linux-X64-ubuntu24-01ee",
            "dispatch-rust-binary-debug-Linux-X64-1dd",
            "v0-rust-2026-W45-dispatch-release-Linux-x64-019b21e0-00000002",
            "v0-rust-2026-W45-dispatch-debug-Linux-x64-019b21e0-00000005",
        ])

    def test_keeps_what_was_read_lately_and_other_refs_caches(self):
        listed = [
            # Main back on its first lockfile read it today, and a PR the second yesterday.
            *entries("dispatch-node-modules-Linux-X64-{}aa", 1, read=20),
            *entries("dispatch-node-modules-Linux-X64-{}aa", 2, read=19),
            *entries("dispatch-node-modules-Linux-X64-{}aa", 3, 4, 5),
            *entries("dispatch-node-modules-Linux-X64-{}aa", 1, 2, 3, ref="refs/heads/release-checks/v1"),
            # CodeQL's, which only main's analyses write, saved today: the write is all that
            # read them.
            *entries(
                "codeql-overlay-base-database-1-58b1deea8a9e87c1-javascript-2.27.4-"
                f"{'b' * 40}-4177{{}}-1",
                1, 2, 3, read=20,
            ),
        ]
        self.assertEqual(
            [cache.key for cache in caches.superseded(listed, NOW)],
            [
                f"codeql-overlay-base-database-1-58b1deea8a9e87c1-javascript-2.27.4-{'b' * 40}-41771-1",
                "dispatch-node-modules-Linux-X64-3aa",
            ],
        )

    def test_deletes_closed_prs_caches(self):
        listed = [
            *entries("dispatch-tools-Linux-X64-ubuntu24-{}ee", 1, 2, ref="refs/pull/7/merge"),
            *entries("dispatch-tools-Linux-X64-ubuntu24-{}ee", 3, ref="refs/pull/8/merge"),
            *entries("dispatch-tools-Linux-X64-ubuntu24-{}ee", 4, ref="refs/pull/9/merge"),
        ]
        states = {7: "closed", 8: "open", 9: None}
        self.assertEqual([cache.id for cache in caches.closed(listed, states.get)], [1, 2])

    def test_reads_every_cache_as_json_and_deletes_by_id(self):
        calls = []
        # A PR's workflow chose this key, line breaks and all.
        forged = {"id": 5, "key": "owned\n2\tforged", "ref": "refs/pull/7/merge",
                  "created_at": "2026-11-01T00:00:00Z", "last_accessed_at": "2026-11-01T00:00:00Z"}
        listing = [{"actions_caches": [forged]}]

        def run(args):
            calls.append(args)
            if "--paginate" in args:
                return json.dumps(listing)
            return "closed\n" if args[2].endswith("/pulls/7") else ""

        with contextlib.redirect_stdout(io.StringIO()):
            caches.prune("example/repo", dry_run=False, run=run)
        self.assertEqual(calls[-1], ["gh", "api", "-X", "DELETE", "repos/example/repo/actions/caches/5"])
        self.assertEqual(sum("DELETE" in call for call in calls), 1)


if __name__ == "__main__":
    unittest.main()
