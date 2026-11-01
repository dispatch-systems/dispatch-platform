import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import stat
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("relay_ci", ROOT / "tooling/ci/relay.py")
relay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(relay)

REPOSITORY = "example/relay"
APK = b"production apk"
# The signing certificate, as its bytes, and its SHA-256.
CERTIFICATE = b"the dev certificate"
CERT = hashlib.sha256(CERTIFICATE).hexdigest()


def printed(*certificates: bytes, label: str = "V2 Signer:") -> bytes:
    """`apksigner verify --print-certs-pem`'s output for an APK signed with `certificates`."""
    return "".join(
        f"{label} certificate DN: CN=DSP Dispatch Dev\n"
        f"-----BEGIN CERTIFICATE-----\n{base64.b64encode(der).decode()}\n"
        "-----END CERTIFICATE-----\n"
        for der in certificates
    ).encode()
SHA = hashlib.sha256(APK).hexdigest()


def released(version="1.4.0", digest=f"sha256:{SHA}", entry=None):
    """GitHub's answers for a latest release carrying Production's `version`."""
    entry = entry or {
        "version": version,
        "versionCode": 790,
        "commit": "1" * 40,
        "asset": f"dsp-dispatch-relay-{version}.apk",
        "sha256": SHA,
        "size": len(APK),
    }
    builds = json.dumps({"production": entry}).encode()
    release = {
        "tag_name": "v0.0.40",
        "assets": [
            {"id": 1, "name": "relay.json", "size": len(builds),
             "digest": f"sha256:{hashlib.sha256(builds).hexdigest()}"},
            {"id": 2, "name": entry["asset"], "size": len(APK), "digest": digest},
        ],
    }
    return {
        f"repos/{REPOSITORY}/releases/latest": json.dumps(release).encode(),
        f"repos/{REPOSITORY}/releases/assets/1": builds,
        f"repos/{REPOSITORY}/releases/assets/2": APK,
    }


class Commands:
    """Answers each command as `answers` has it, whole or by its last argument, and records
    them."""

    def __init__(self, answers):
        self.answers = answers
        self.ran = []

    def __call__(self, args):
        self.ran.append(args)
        answer = self.answers.get(" ".join(args), self.answers.get(args[-1]))
        if answer is None:
            raise relay.Refused(f"{args[0]} failed: gh: Not Found (HTTP 404)")
        return answer


GIT = {
    "git rev-list --count HEAD": b"812\n",
    "git rev-parse HEAD": b"2" * 40 + b"\n",
    "git log -1 --format=%H HEAD -- android": b"3" * 40 + b"\n",
}


class RelayCiTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.folder = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def test_dev_is_named_after_the_next_app_version_and_the_commit_count(self):
        named = relay.version("dev", None, REPOSITORY, Commands({**released("1.4.0"), **GIT}))
        self.assertEqual(named, {
            "commit": "2" * 40, "code": "812", "name": "1.4.1-dev.812", "next": "1.4.1",
            "changed": "3" * 40,
        })
        # Before any release carried the app, the first is 0.0.1: none at all, or one without.
        self.assertEqual(relay.version("dev", None, REPOSITORY, Commands(GIT))["name"],
                         "0.0.1-dev.812")
        bare = {f"repos/{REPOSITORY}/releases/latest": b'{"tag_name": "v0.0.40", "assets": []}'}
        self.assertEqual(relay.version("dev", None, REPOSITORY, Commands({**bare, **GIT}))["next"],
                         "0.0.1")

    def test_production_is_named_the_version_asked_for_and_only_a_release_version(self):
        answers = Commands({**released("1.4.0"), **GIT})
        self.assertEqual(relay.version("production", "1.5.0", REPOSITORY, answers)["name"], "1.5.0")
        for wanted in [None, "", "1.5", "1.05.0", "1.5.0-dev.812", "1.5.0\n"]:
            with self.assertRaises(relay.Refused, msg=wanted):
                relay.version("production", wanted, REPOSITORY, answers)

    def test_a_failure_other_than_no_release_stops_naming_the_build(self):
        def offline(args):
            raise relay.Refused("gh failed: connection refused")

        with self.assertRaises(relay.Refused):
            relay.version("dev", None, REPOSITORY, offline)

    def test_a_failed_command_says_its_error_and_never_its_arguments(self):
        with self.assertRaises(relay.Refused) as refused:
            relay.run([sys.executable, "-c", "import sys; sys.exit('bad password')", "s3cret"])
        self.assertIn("bad password", str(refused.exception))
        self.assertNotIn("s3cret", str(refused.exception))

    def test_the_key_is_written_for_its_owner_alone_and_named_by_its_certificate(self):
        file = self.folder / "relay.p12"
        env = {"RELAY_KEYSTORE": base64.encodebytes(b"keystore").decode(), "RELAY_KEY_ALIAS": "a"}
        answers = Commands({"a": b"certificate"})
        self.assertEqual(relay.key(file, env, answers), hashlib.sha256(b"certificate").hexdigest())
        self.assertEqual(file.read_bytes(), b"keystore")
        self.assertEqual(stat.S_IMODE(file.stat().st_mode), 0o600)
        # The password reaches keytool by its variable's name, never its value.
        self.assertIn("RELAY_KEYSTORE_PASSWORD", answers.ran[0])
        with self.assertRaises(relay.Refused):
            relay.key(self.folder / "other.p12", {"RELAY_KEYSTORE": "not base64!"}, answers)

    def test_a_failure_reports_the_programs_own_error_and_never_its_arguments(self):
        for script in ["print('keytool error: no such keystore'); exit(1)",
                       "import sys; sys.stderr.write('keytool error: no such keystore'); exit(1)"]:
            with self.assertRaises(relay.Refused) as refused:
                relay.run([sys.executable, "-c", script, "secret-argument"])
            self.assertIn("keytool error: no such keystore", str(refused.exception))
            self.assertNotIn("secret-argument", str(refused.exception))

    def test_an_entry_names_its_apk_as_its_asset_and_is_checked_against_it(self):
        apk = self.folder / "relay-dev-release.apk"
        apk.write_bytes(b"dev apk")
        apps = self.folder / "apps"
        made = relay.entry("dev", apk, apps, "2" * 40, 812, "1.4.1-dev.812")
        self.assertEqual(made, {
            "version": "1.4.1-dev.812", "versionCode": 812, "commit": "2" * 40,
            "asset": "dsp-dispatch-relay-1.4.1-dev.812.apk",
            "sha256": hashlib.sha256(b"dev apk").hexdigest(), "size": 7,
        })
        self.assertEqual(json.loads((apps / "dev.json").read_text()), made)
        tools = self.folder / "sdk/build-tools/36.1.0"
        tools.mkdir(parents=True)
        (tools / "apksigner").touch()
        (tools / "aapt2").touch()
        env = {"ANDROID_HOME": str(self.folder / "sdk")}
        signed = printed(CERTIFICATE)
        badging = b"package: name='com.dspdispatch.relay.dev' versionCode='812' " \
                  b"versionName='1.4.1-dev.812' platformBuildVersionName='17'\n"
        path = str(apps / made["asset"])
        answers = {path: signed, "badging": badging}

        def commands(args):
            return answers["badging"] if args[1] == "dump" else answers[args[-1]]

        self.assertEqual(relay.signed(apps, "dev", CERT, env, commands), made)
        # Whatever the tool's version calls its signers, and however many schemes sign with the key.
        for answer in [printed(CERTIFICATE, label="Signer #1"),
                       printed(CERTIFICATE) + printed(CERTIFICATE, label="V3 Signer:")]:
            answers[path] = answer
            self.assertEqual(relay.signed(apps, "dev", CERT, env, commands), made)
        # Another key, a second signer, or another app or version is refused.
        for answer in [printed(b"another certificate"),
                       printed(CERTIFICATE, b"another certificate"),
                       b"Signer #1 certificate SHA-256 digest: " + CERT.encode()]:
            answers[path] = answer
            with self.assertRaises(relay.Refused):
                relay.signed(apps, "dev", CERT, env, commands)
        answers[path] = signed
        for other in [badging.replace(b".relay.dev'", b".relay'"),
                      badging.replace(b"versionCode='812'", b"versionCode='811'"),
                      badging.replace(b"versionName='1.4.1-dev.812'", b"versionName='1.4.1-dev.811'")]:
            answers["badging"] = other
            with self.assertRaises(relay.Refused):
                relay.signed(apps, "dev", CERT, env, commands)
        answers["badging"] = badging
        # So is an APK unlike its entry, in its size or, the same size, its bytes.
        for other in [b"other apk", b"dev apK"]:
            (apps / made["asset"]).write_bytes(other)
            with self.assertRaises(relay.Refused):
                relay.signed(apps, "dev", CERT, env, commands)

    def test_a_release_carries_the_last_released_production_build_byte_for_byte(self):
        made = relay.carry(self.folder, REPOSITORY, Commands(released("1.4.0")))
        self.assertEqual(made["version"], "1.4.0")
        self.assertEqual((self.folder / "dsp-dispatch-relay-1.4.0.apk").read_bytes(), APK)
        self.assertEqual(json.loads((self.folder / "production.json").read_text()), made)

    def test_nothing_is_carried_before_a_release_carries_the_app(self):
        self.assertIsNone(relay.carry(self.folder, REPOSITORY, Commands({})))
        self.assertEqual(list(self.folder.iterdir()), [])

    def test_a_carried_apk_unlike_its_entry_or_githubs_digest_is_refused(self):
        entry = {
            "version": "1.4.0", "versionCode": 790, "commit": "1" * 40,
            "asset": "dsp-dispatch-relay-1.4.0.apk", "sha256": SHA, "size": len(APK),
        }
        tampered = {**released("1.4.0"), f"repos/{REPOSITORY}/releases/assets/2": b"other apk!!!!!"}
        for answers in [
            released("1.4.0", digest="sha256:" + "0" * 64),
            tampered,
            released(entry={**entry, "size": len(APK) + 1}),
            released(entry={**entry, "sha256": "0" * 64}),
            # An asset named as no release names its own, outside the folder here.
            released(entry={**entry, "asset": "../relay.apk"}),
        ]:
            with self.assertRaises(relay.Refused):
                relay.carry(self.folder, REPOSITORY, Commands(answers))
        self.assertEqual(list(self.folder.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
