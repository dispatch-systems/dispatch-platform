import contextlib
import importlib.util
import io
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("caches_ci", ROOT / "tooling/ci/caches.py")
caches = importlib.util.module_from_spec(spec)
spec.loader.exec_module(caches)
Cache = caches.Cache


def entries(key: str, *days: int) -> list:
    """`key`'s entries saved on each of `days`, `{}` in `key` taking the day."""
    return [Cache(day, key.format(day), f"2026-11-{day:02d}T00:00:00Z") for day in days]


class PruneTest(unittest.TestCase):
    def test_deletes_only_what_newer_ones_of_the_same_kind_replaced(self):
        listed = [
            *entries("v0-rust-2026-W45-dispatch-release-Linux-x64-019b21e0-0000000{}", 1, 4, 2, 3),
            # Debug's entries are another kind: release's newer ones replace none of them.
            *entries("v0-rust-2026-W45-dispatch-debug-Linux-x64-019b21e0-0000000{}", 5, 6),
            *entries("v0-rust-2026-W46-dispatch-debug-Linux-x64-019b21e0-0000000{}", 7),
            *entries("dispatch-playwright-Linux-X64-1.6{}.0", 1, 2, 3),
            *entries("dispatch-node-modules-Linux-X64-{}aa", 1, 2, 3),
            *entries(
                "codeql-overlay-base-database-1-58b1deea8a9e87c1-javascript-2.27.4-"
                f"{'b' * 40}-4177{{}}-1",
                1, 2, 3,
            ),
            # The tools keep ten, each kind and image apart.
            *entries("dispatch-tools-Linux-X64-ubuntu24-{:02d}ee", *range(1, 12)),
            *entries("dispatch-fixture-Linux-X64-ubuntu24-{:02d}ee", 1, 2),
            # No kind: the exact binaries, Gradle's own, and whatever comes next.
            *entries("dispatch-rust-binary-Linux-X64-{}bb", 1, 2, 3),
            *entries("gradle-transforms-v2-{}cc", 1, 2, 3),
        ]
        deleted = [cache.key for cache in caches.superseded(listed)]
        self.assertEqual(deleted, [
            "v0-rust-2026-W45-dispatch-release-Linux-x64-019b21e0-00000001",
            "dispatch-playwright-Linux-X64-1.61.0",
            "dispatch-node-modules-Linux-X64-1aa",
            f"codeql-overlay-base-database-1-58b1deea8a9e87c1-javascript-2.27.4-{'b' * 40}-41771-1",
            "dispatch-tools-Linux-X64-ubuntu24-01ee",
            "v0-rust-2026-W45-dispatch-release-Linux-x64-019b21e0-00000002",
            "v0-rust-2026-W45-dispatch-debug-Linux-x64-019b21e0-00000005",
        ])

    def test_reads_main_alone_and_deletes_by_id(self):
        calls = []

        def run(args):
            calls.append(args)
            if "--paginate" in args:
                return "".join(
                    f"{cache.id}\t{cache.key}\t{cache.created}\n"
                    for cache in entries("dispatch-browseros-{}dd", 1, 2, 3)
                )
            return ""

        with contextlib.redirect_stdout(io.StringIO()):
            caches.prune("example/repo", dry_run=False, run=run)
        self.assertIn("ref=refs/heads/main", calls[0][3])
        self.assertEqual(calls[1:], [["gh", "api", "-X", "DELETE", "repos/example/repo/actions/caches/1"]])


if __name__ == "__main__":
    unittest.main()
