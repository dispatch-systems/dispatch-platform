import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("monitor", Path(__file__).resolve().parents[2] / "ops/scripts/security-monitor.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class MonitorTests(unittest.TestCase):
    def test_alert_transport_refuses_redirects(self):
        handler = module.NoRedirects()
        self.assertIsNone(
            handler.redirect_request(None, None, 302, "moved", {}, "https://elsewhere.test")
        )

    def test_thresholds_expire_and_output_never_contains_payloads(self):
        monitor = module.Monitor()
        event = {"event": "http.request", "fields": {
            "route": "/api/auth/login", "status": 401, "account": "opaque-account",
            "password": "do-not-log", "client": "invalid address", "body": "private",
        }}
        for at in range(9):
            self.assertEqual(monitor.observe(event, at), [])
        alert = monitor.observe(event, 9)
        self.assertEqual(alert, [{"rule": "authentication_failures", "windowSeconds": 300, "account": "opaque-account"}])
        self.assertEqual(monitor.observe(event, 10), [])
        self.assertEqual(monitor.observe(event, 400), [])

    def test_bulk_reads_and_sensitive_changes(self):
        monitor = module.Monitor()
        event = {"event": "http.request", "fields": {"route": "/api/dsp/employees", "status": 200, "bulk": True, "actorId": "usr_test"}}
        for at in range(4):
            self.assertEqual(monitor.observe(event, at), [])
        self.assertEqual(monitor.observe(event, 4)[0]["rule"], "bulk_reads")
        event["fields"].update(route="/api/dsp/members/{id}", method="POST", bulk=False)
        self.assertEqual(monitor.observe(event, 5)[0]["rule"], "access_changed")

    def test_oauth_abuse_and_saturation_are_globally_bounded(self):
        monitor = module.Monitor()
        failed = {"event": "http.request", "fields": {
            "route": "/oauth/token", "status": 401, "method": "POST",
        }}
        alerts = []
        for second in range(25):
            alerts.extend(monitor.observe(failed, second))
        self.assertEqual(alerts, [{"rule": "oauth_failures", "windowSeconds": 60}])

        busy = {"event": "http.request", "fields": {
            "route": "/oauth/revoke", "status": 503, "method": "POST",
        }}
        alerts = []
        for second in range(3):
            alerts.extend(monitor.observe(busy, 100 + second))
        self.assertTrue(any(alert["rule"] == "oauth_saturation" for alert in alerts))

    def test_oauth_authorization_and_registration_warn_before_capacity_is_lost(self):
        monitor = module.Monitor()
        limited = {"event": "http.request", "fields": {
            "route": "/oauth/authorize", "status": 303, "error": "rate_limited",
            "method": "GET",
        }}
        alerts = []
        for second in range(5):
            alerts.extend(monitor.observe(limited, second))
        self.assertEqual(alerts, [{
            "rule": "oauth_authorization_pressure", "windowSeconds": 300,
        }])

        registered = {"event": "http.request", "fields": {
            "route": "/oauth/register", "status": 201, "method": "POST",
        }}
        alerts = []
        for second in range(20):
            alerts.extend(monitor.observe(registered, 100 + second))
        self.assertEqual(alerts, [{
            "rule": "oauth_registration_volume", "windowSeconds": 300,
        }])

    def test_remote_mail_contains_only_allowlisted_opaque_fields(self):
        calls = []

        class Response:
            status = 200
            def __enter__(self): return self
            def __exit__(self, *_): return None
            def read(self, limit): return b'{"ok":true}'[:limit]

        def send(request, timeout):
            calls.append((request, timeout))
            return Response()

        sink = module.MailAlerts({
            "DISPATCH_SECURITY_ALERT_URL": "https://mail.example.test/send",
            "DISPATCH_SECURITY_ALERT_TOKEN": "x" * 32,
            "DISPATCH_SECURITY_ALERT_TO": "security@example.test",
            "DISPATCH_ENVIRONMENT": "production",
            "DISPATCH_ORIGIN": "https://dispatch.example.test",
        }, send)
        sink.deliver({"rule": "bulk_reads", "actorId": "usr_opaque", "password": "secret"})
        request, timeout = calls[0]
        self.assertEqual(timeout, 5)
        self.assertEqual(request.get_header("Authorization"), "Bearer " + "x" * 32)
        message = json.loads(request.data)
        self.assertEqual(message["to"], "security@example.test")
        self.assertIn('"actorId":"usr_opaque"', message["text"])
        self.assertNotIn("password", message["text"])
        self.assertNotIn("secret", message["text"])

    def test_remote_mail_rejects_plaintext_and_incomplete_configuration(self):
        valid = {
            "DISPATCH_SECURITY_ALERT_URL": "https://mail.example.test/send",
            "DISPATCH_SECURITY_ALERT_TOKEN": "x" * 32,
            "DISPATCH_SECURITY_ALERT_TO": "security@example.test",
            "DISPATCH_ENVIRONMENT": "production",
            "DISPATCH_ORIGIN": "https://dispatch.example.test",
        }
        for key, value in [
            ("DISPATCH_SECURITY_ALERT_URL", "http://mail.example.test/send"),
            ("DISPATCH_SECURITY_ALERT_TOKEN", "short"),
            ("DISPATCH_SECURITY_ALERT_TO", "not-an-address"),
            ("DISPATCH_ORIGIN", "http://dispatch.example.test"),
        ]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                module.MailAlerts({**valid, key: value})


if __name__ == "__main__":
    unittest.main()
