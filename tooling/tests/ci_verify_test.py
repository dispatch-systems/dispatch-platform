"""The gate job's artifact check hands the archive to the host manager."""
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "ops/launchers"))


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tooling/ci" / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


verify = module("ci_verify", "ci-verify.py")


class CiVerifyTests(unittest.TestCase):
    def test_verify_hands_the_archive_to_the_checkout_host(self):
        with patch.object(verify, "host_binary", return_value=Path("/tmp/host")), \
                patch.object(verify.os, "execv") as execute:
            verify.main(["/tmp/artifact with spaces.tar.gz"])
            execute.assert_called_once_with("/tmp/host", ["/tmp/host", "host", "ci", "verify", "/tmp/artifact with spaces.tar.gz"])


if __name__ == "__main__":
    unittest.main()
