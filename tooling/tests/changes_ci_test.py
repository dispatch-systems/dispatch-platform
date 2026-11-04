import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("changes_ci", ROOT / "tooling/ci/changes.py")
changes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(changes)

EVERY = set(changes.SUITES)


class NeededTest(unittest.TestCase):
    def assertRuns(self, paths, suites, pull_request=True):
        found, _ = changes.needed(paths, pull_request)
        self.assertEqual({suite for suite, runs in found.items() if runs}, suites, paths)

    def test_a_suite_skips_only_when_every_change_is_of_a_kind_it_never_reads(self):
        for paths, suites in [
            (["android/relay/build.gradle.kts"], {"android"}),
            (["README.md", "features/dvic/README.md"], {"api", "scaffold"}),
            (["features/dvic/frontend/DvicPage.tsx"], EVERY - {"collectors", "android"}),
            (["app/frontend/main.tsx", "README.md"], EVERY - {"collectors", "android"}),
            (["android/README.md", "README.md"], {"android", "api", "scaffold"}),
            # The support worker ships its skills' Markdown.
            (["services/support-worker/hermes/skills/dispatch-context/SKILL.md"], EVERY - {"android"}),
            # Only a folder named frontend holds frontend code.
            (["features/dvic/backend/frontend.rs"], EVERY - {"android"}),
            (["features/dvic/backend/dvic.rs", "android/app/x.kt"], EVERY),
            (["tooling/ci/changes.py"], EVERY),
            ([".github/workflows/checks.yml"], EVERY),
        ]:
            self.assertRuns(paths, suites)

    def test_a_push_runs_every_suite_and_the_android_apps_only_for_their_change(self):
        self.assertRuns(["README.md"], EVERY - {"android"}, pull_request=False)
        self.assertRuns(["android/relay/build.gradle.kts"], EVERY, pull_request=False)

    def test_every_suite_runs_when_the_change_is_unknown(self):
        for pull_request in (True, False):
            self.assertRuns(None, EVERY, pull_request)
            self.assertRuns([], EVERY, pull_request)


if __name__ == "__main__":
    unittest.main()
