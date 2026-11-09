import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("nextest", ROOT / "tooling/ci/nextest.py")
nextest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nextest)


def gateway(code):
    return urllib.error.HTTPError(
        "https://example.test/archive", code, "Download failed", {}, io.BytesIO(b"temporary")
    )


class NextestDownloadTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.folder = Path(directory.name) / "downloads"
        self.data = b"archive from the pinned release"
        self.pin = {"version": "fixture", "archives": {
            "linux_x64": hashlib.sha256(self.data).hexdigest()}}

    def test_recovers_from_gateway_failures_then_reuses_verified_archive(self):
        with patch.object(nextest.urllib.request, "urlopen",
                          side_effect=[gateway(504), gateway(502), io.BytesIO(self.data)]) as opened:
            self.assertEqual(nextest.archive(self.pin, "linux_x64", self.folder), self.data)
            self.assertEqual(nextest.archive(self.pin, "linux_x64", self.folder), self.data)
        self.assertEqual(opened.call_count, 3)

    def test_stops_after_three_gateway_failures_without_caching(self):
        with patch.object(nextest.urllib.request, "urlopen",
                          side_effect=[gateway(504) for _ in range(3)]) as opened:
            with self.assertRaises(urllib.error.HTTPError) as failed:
                nextest.archive(self.pin, "linux_x64", self.folder)
        self.assertEqual(failed.exception.code, 504)
        self.assertEqual(opened.call_count, 3)
        self.assertFalse(self.folder.exists())

    def test_permission_and_missing_release_errors_are_not_retried(self):
        for code in (403, 404):
            with self.subTest(code=code):
                with patch.object(nextest.urllib.request, "urlopen",
                                  side_effect=gateway(code)) as opened:
                    with self.assertRaises(urllib.error.HTTPError) as failed:
                        nextest.archive(self.pin, "linux_x64", self.folder)
                self.assertEqual(failed.exception.code, code)
                self.assertEqual(opened.call_count, 1)
                self.assertFalse(self.folder.exists())

    def test_retry_does_not_accept_a_different_archive(self):
        with patch.object(nextest.urllib.request, "urlopen",
                          side_effect=[gateway(504), io.BytesIO(b"different archive")]) as opened:
            with self.assertRaisesRegex(SystemExit, "archive checksum mismatch"):
                nextest.archive(self.pin, "linux_x64", self.folder)
        self.assertEqual(opened.call_count, 2)
        self.assertFalse(self.folder.exists())


if __name__ == "__main__":
    unittest.main()
