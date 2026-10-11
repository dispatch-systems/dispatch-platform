"""Production's reader runs only its fixed reads, and app logs show one DSP's events."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import pwd
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("reader", Path(__file__).resolve().parents[2] / "ops/scripts/production-reader.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
NOW = 1_800_000_000
DSP = "dsp_" + "a" * 32
OTHER = "dsp_" + "b" * 32
SETTINGS = argparse.Namespace(owner=pwd.getpwuid(os.getuid()).pw_name, health="http://127.0.0.1:5180/api/health")


def entry(seconds_ago, message):
    return {"__REALTIME_TIMESTAMP": str((NOW - seconds_ago) * 1_000_000),
            "MESSAGE": message if isinstance(message, str) else json.dumps(message)}


def request(dsp, actor, route, level="info"):
    return {"level": level, "event": "http.request", "fields": {
        "requestId": "req_" + "1" * 16, "route": route, "dspId": dsp, "actorId": actor,
        "client": "CLIENT_CANARY", "account": "ACCOUNT_CANARY", "status": 500 if level == "error" else 200}}


class ReaderTests(unittest.TestCase):
    def test_requests_off_the_fixed_list_are_refused_before_anything_is_read(self):
        refused = [[], ["cat", "/etc/shadow"], ["health", "--since", "1h"], ["logs", "--since"],
                   ["logs", "--since", "1h", "--since", "2h"], ["logs", "--path", "/etc"],
                   ["logs", "--since", "8d"], ["logs", "--since", "1h", "--until", "2h"],
                   ["logs", "--dsp", "dsp_x;id"], ["logs", "--route", "/api x"],
                   ["logs", "--service", "updater", "--dsp", DSP], ["logs", "--service", "shell"],
                   ["logs", "--level", "debug"]]
        with mock.patch.object(module, "journal") as journal, mock.patch.object(module, "health") as health:
            for words in refused:
                with self.subTest(words=words), self.assertRaises(module.Refused):
                    module.answer(words, SETTINGS, NOW)
        journal.assert_not_called()
        health.assert_not_called()

    def test_app_logs_show_one_dsp_and_platform_events_without_client_identifiers(self):
        entries = [
            entry(50, request(DSP, "usr_mine", "/api/dsp/routes", "error")),
            entry(40, request(OTHER, "usr_other", "/api/dsp/routes", "error")),
            entry(30, request(None, None, "/wp-login.php", "warn")),
            entry(20, request(None, "usr_owner", "/api/platform/dsps")),
            entry(10, {"level": "warn", "event": "database.slow", "fields": {"workMs": 900}}),
            entry(5, "thread 'main' panicked; write to someone@example.com"),
        ]
        with mock.patch.object(module, "journal", side_effect=lambda arguments: iter(entries)) as journal:
            shown = module.answer(["logs", "--since", "1h", "--dsp", DSP], SETTINGS, NOW)
            text = "\n".join(shown["entries"])
            self.assertEqual(shown["matched"], 3)
            for seen in ("usr_mine", "database.slow", "panicked", "[redacted]"):
                self.assertIn(seen, text)
            for hidden in ("usr_other", "wp-login", "usr_owner", "CLIENT_CANARY", "ACCOUNT_CANARY", "someone@example.com"):
                self.assertNotIn(hidden, text)
            self.assertIn("_SYSTEMD_USER_UNIT=dispatch-production.service", journal.call_args.args[0])
            # A panic is kept at any level; a route or request leaves only the matching requests.
            self.assertEqual(module.answer(["logs", "--dsp", DSP, "--level", "error"], SETTINGS, NOW)["matched"], 2)
            routed = module.answer(["logs", "--dsp", DSP, "--route", "/api/dsp"], SETTINGS, NOW)
            self.assertEqual(routed["matched"], 1)
            self.assertIn("usr_mine", routed["entries"][0])
            # With no DSP, only platform events: no request of any DSP or account.
            platform = "\n".join(module.answer(["logs"], SETTINGS, NOW)["entries"])
            self.assertIn("database.slow", platform)
            self.assertNotIn("usr_", platform)


if __name__ == "__main__":
    unittest.main()
