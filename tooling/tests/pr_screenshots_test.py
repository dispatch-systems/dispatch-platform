import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

SOURCE = Path(__file__).resolve().parents[2] / "tooling/screenshots/pr-screenshots.py"
spec = importlib.util.spec_from_file_location("pr_screenshots", SOURCE)
shots = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shots)

PNG = b"\x89PNG\r\n\x1a\n synthetic"
SETTINGS = {"endpoint": "https://account.r2.example", "bucket": "shots", "accessKeyId": "synthetic-key-id",
            "secretAccessKey": "synthetic-secret", "publicUrl": "https://shots.example/"}


def git(*args, cwd, env=None):
    return subprocess.run(["git", *args], cwd=cwd, text=True, capture_output=True, check=True,
                          env={**os.environ, **(env or {})}).stdout.strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


class Opener:
    """Stands in for urlopen, keeping each request it is handed."""

    def __init__(self, error=None):
        self.requests, self.error = [], error

    def __call__(self, request, timeout):
        if self.error:
            raise self.error
        self.requests.append(request)
        return io.BytesIO(b"")


class PrScreenshotsTests(unittest.TestCase):
    """A worktree on a PR branch; uploads go to a stand-in, never to R2."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        base = Path(self.directory.name)
        self.root = base / "workspace/worktrees/pr-flow"
        self.root.mkdir(parents=True)
        self._tree = None
        self.scratch = base / "scratch"
        self.review = base / "workspace/.privacy/export-review.json"
        self.review.parent.mkdir(parents=True)
        self.review.write_text(json.dumps({"assets": {}}))
        self.review.chmod(0o600)
        self.settings = base / "upload.json"
        self.env = patch.dict(os.environ, {"DISPATCH_SCRATCH": str(self.scratch),
                                           "DISPATCH_SCREENSHOTS_SETTINGS": str(self.settings)})
        self.env.start()
        self.addCleanup(self.env.stop)
        self.uploaded = []

    @property
    def tree(self):
        if self._tree is None:
            git("init", "-q", "-b", "pr-flow", cwd=self.root)
            git("commit", "-q", "--allow-empty", "-m", "start", cwd=self.root,
                env={"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@example.com",
                     "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@example.com"})
            self._tree = shots.Worktree(self.root)
        return self._tree

    def uploader(self, settings, captured):
        self.uploaded.extend(f"{label}/{name}" for label, name, _ in captured)
        # Where each image came from, and whether it was still there to upload.
        self.uploaded_from = [(path, path.is_file()) for _, _, path in captured]
        return {(label, name): f"https://shots.example/{label}-{name}" for label, name, _ in captured}

    def capture(self, label, *names):
        directory = self.scratch / "screenshots" / label
        directory.mkdir(parents=True, exist_ok=True)
        for name in names:
            (directory / f"{name}.png").write_bytes(PNG + name.encode() + label.encode())
        (directory / "index.json").write_text(json.dumps({"team": "Team & Roles"}))

    def test_section_puts_each_screen_in_its_own_collapsed_dropdown(self):
        captured = [("before", "team.png", None), ("before", "old.png", None), ("after", "team.png", None),
                    ("after", "dsps.png", None)]
        urls = {(label, name): f"https://shots.example/{label}-{name}" for label, name, _ in captured}
        text = shots.section(captured, urls, {"team": "Team & Roles"})
        self.assertEqual(text.splitlines(), [
            "## Screenshots", "",
            "<details>", "<summary>Team &amp; Roles, before and after</summary>", "",
            "Before", "", "![Team & Roles before](https://shots.example/before-team.png)", "",
            "After", "", "![Team & Roles after](https://shots.example/after-team.png)", "",
            "</details>", "",
            "<details>", "<summary>Old, removed</summary>", "",
            "![Old before](https://shots.example/before-old.png)", "",
            "</details>", "",
            "<details>", "<summary>Dsps, new</summary>", "",
            "![Dsps after](https://shots.example/after-dsps.png)", "",
            "</details>",
        ])

    def test_publish_stops_until_the_images_are_reviewed_then_records_and_uploads(self):
        self.capture("before", "team")
        self.capture("after", "team")
        calls = []

        def auditor(tree, staged, review_file, tesseract):
            calls.append(sorted(p.relative_to(staged).as_posix() for p in staged.rglob("*.png")))
            approved = json.loads(review_file.read_text())["assets"]
            return {name: {shots.NEEDS_REVIEW} for name in
                    ["screenshots/pr-flow/before/team.png", "screenshots/pr-flow/after/team.png"]
                    if name not in approved}

        with self.assertRaises(SystemExit) as stop:
            shots.publish(self.tree, False, self.review, None, SETTINGS, self.uploader, auditor)
        self.assertIn("--reviewed", str(stop.exception))
        self.assertIn(str(self.scratch / "screenshots/after/team.png"), str(stop.exception))
        self.assertEqual(self.uploaded, [])

        with patch("sys.stdout"):
            text = shots.publish(self.tree, True, self.review, None, SETTINGS, self.uploader, auditor)
        self.assertEqual(calls[0], ["screenshots/pr-flow/after/team.png", "screenshots/pr-flow/before/team.png"])
        manifest = json.loads(self.review.read_text())
        self.assertEqual(sorted(manifest["assets"]),
                         ["screenshots/pr-flow/after/team.png", "screenshots/pr-flow/before/team.png"])
        self.assertEqual(self.review.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.uploaded, ["before/team.png", "after/team.png"])
        # The audited copies are uploaded, not the captures, which could have changed since.
        self.assertTrue(all(present and self.scratch not in path.parents for path, present in self.uploaded_from))
        self.assertEqual((self.scratch / "screenshots/section.md").read_text(), text)
        self.assertIn("<summary>Team &amp; Roles, before and after</summary>", text)
        self.assertIn("![Team & Roles after](https://shots.example/after-team.png)", text)

    def test_upload_puts_each_distinct_image_once_under_its_sha256(self):
        self.capture("after", "team", "dsps")
        same = self.scratch / "screenshots/before/team.png"
        same.parent.mkdir(parents=True)
        same.write_bytes((self.scratch / "screenshots/after/team.png").read_bytes())
        opener = Opener()
        urls = shots.upload(SETTINGS, shots.files(self.tree), opener)
        team, dsps = digest(PNG + b"teamafter"), digest(PNG + b"dspsafter")
        self.assertEqual(urls, {("before", "team.png"): f"https://shots.example/{team}.png",
                                ("after", "team.png"): f"https://shots.example/{team}.png",
                                ("after", "dsps.png"): f"https://shots.example/{dsps}.png"})
        self.assertEqual([(r.method, r.full_url) for r in opener.requests],
                         [("PUT", f"https://account.r2.example/shots/{team}.png"),
                          ("PUT", f"https://account.r2.example/shots/{dsps}.png")])
        request = opener.requests[0]
        self.assertEqual(request.data, PNG + b"teamafter")
        self.assertEqual(request.get_header("Content-type"), "image/png")
        self.assertEqual(request.get_header("Cache-control"), "public, max-age=31536000, immutable")
        self.assertEqual(request.get_header("X-amz-content-sha256"), team)
        authorization = request.get_header("Authorization")
        stamp = request.get_header("X-amz-date")
        self.assertTrue(authorization.startswith(
            f"AWS4-HMAC-SHA256 Credential=synthetic-key-id/{stamp[:8]}/auto/s3/aws4_request, "
            "SignedHeaders=cache-control;content-type;host;x-amz-content-sha256;x-amz-date, Signature="))
        self.assertNotIn("synthetic-secret", authorization)

    def test_upload_failure_names_r2s_error_code_only(self):
        self.capture("after", "team")
        body = io.BytesIO(b"<Error><Code>AccessDenied</Code><Message>synthetic detail</Message></Error>")
        error = urllib.error.HTTPError("https://account.r2.example", 403, "Forbidden", {}, body)
        with self.assertRaises(SystemExit) as stop:
            shots.upload(SETTINGS, shots.files(self.tree), Opener(error))
        self.assertEqual(str(stop.exception), "Uploading after/team.png failed: HTTP 403 AccessDenied")

    def test_signing_matches_the_aws_example(self):
        # The GET Object example in AWS's Signature Version 4 documentation for S3.
        empty = digest(b"")
        authorization = shots.sign(
            "GET", "https://examplebucket.s3.amazonaws.com/test.txt",
            {"Range": "bytes=0-9", "x-amz-content-sha256": empty, "x-amz-date": "20130524T000000Z"},
            empty, "AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "us-east-1")
        self.assertEqual(authorization,
                         "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, "
                         "SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, "
                         "Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41")

    def test_upload_settings_must_exist_be_private_and_complete(self):
        with self.assertRaises(SystemExit) as stop:
            shots.upload_settings()
        self.assertIn("No upload settings", str(stop.exception))
        for field in shots.SETTING_FIELDS:
            self.assertIn(field, str(stop.exception))
        self.settings.write_text(json.dumps(SETTINGS))
        self.settings.chmod(0o644)
        with self.assertRaises(SystemExit) as stop:
            shots.upload_settings()
        self.assertIn("mode 0600", str(stop.exception))
        self.settings.chmod(0o600)
        self.assertEqual(shots.upload_settings(), SETTINGS)
        self.settings.write_text(json.dumps({**SETTINGS, "publicUrl": ""}))
        with self.assertRaises(SystemExit) as stop:
            shots.upload_settings()
        self.assertIn("lacks publicUrl", str(stop.exception))

    def test_publish_without_settings_stops_before_the_audit(self):
        self.capture("after", "team")
        audited = []
        with self.assertRaises(SystemExit):
            shots.publish(self.tree, True, self.review, None, auditor=lambda *a: audited.append(a) or {})
        self.assertEqual(audited, [])

    def test_other_findings_stop_publishing_without_printing_values(self):
        self.capture("after", "team")
        auditor = lambda tree, staged, review, tesseract: {
            "screenshots/pr-flow/after/team.png": {"ocr:known-private-identity", shots.NEEDS_REVIEW}}
        with self.assertRaises(SystemExit) as stop:
            shots.publish(self.tree, True, self.review, None, SETTINGS, self.uploader, auditor)
        self.assertIn("ocr:known-private-identity", str(stop.exception))
        self.assertEqual(json.loads(self.review.read_text())["assets"], {})
        self.assertEqual(self.uploaded, [])

    def test_publishing_from_main_is_refused(self):
        self.capture("after", "team")
        self.tree.branch = "main"
        with self.assertRaises(SystemExit) as stop:
            shots.publish(self.tree, True, self.review, None, SETTINGS, self.uploader, lambda *a: {})
        self.assertIn("not main", str(stop.exception))


if __name__ == "__main__":
    unittest.main()
