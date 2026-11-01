#!/usr/bin/env python3
"""DSP Dispatch: Relay's builds in CI (`.github/workflows/checks.yml`): each command one step of
building, signing and packing the app, which `dispatchdev build` then packs from the folder named
in `DISPATCH_RELAY_APPS` ([Android apps](../../android/README.md#releases)).

- `key <file>`: writes the signing key, the base64 PKCS12 in `RELAY_KEYSTORE`, to `file`, which
  only its owner reads, and prints `cert=<SHA-256 of its certificate>` for `$GITHUB_OUTPUT`.
- `version dev` or `version production <X.Y.Z>`: prints the build's names for `$GITHUB_OUTPUT`:
  `commit`, `code` (its versionCode, the commit count), `name` (its versionName), `next` (the app
  version after the last released one, which names Dev's builds) and `changed` (the last commit
  that changed `android/`, so a revert is a new build with a higher versionCode).
- `entry <build> <apk> <folder> <commit> <code> <name>`: copies the build's APK into `folder` as
  its asset, beside `<build>.json`, its entry.
- `signed <folder> <build> <cert>`: checks the APK in `folder` is the one its entry names, the
  build's app of its version, signed by the certificate `cert` alone.
- `carry <folder>`: the Production build the latest release carries, its entry and APK, in
  `folder`, checked against the entry and GitHub's digest of the asset; nothing before any.

Nothing secret is printed: a failure names the command, never its arguments."""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from typing import Callable, Optional

# Each build's app ID.
PACKAGES = {"dev": "com.dspdispatch.relay.dev", "production": "com.dspdispatch.relay"}
# A released app version: three numbers, as 1.4.0, without leading zeros.
RELEASE = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
# The first app version, before any is released.
FIRST = "0.0.1"
# The file a release attaches naming the builds it packed, as `dashboard/relay/` holds it.
RELEASED = "relay.json"
# The largest APK taken, as `dispatchdev build` takes it.
APK_MOST = 200 * 1024 * 1024
# The largest file of builds taken.
RELEASED_MOST = 64 * 1024

# A certificate as `apksigner verify --print-certs-pem` prints it, its base64 inside.
PEM = re.compile(r"-----BEGIN CERTIFICATE-----(.*?)-----END CERTIFICATE-----", re.S)

Run = Callable[[list], bytes]


class Refused(Exception):
    """A step that can't go on, with why."""


def run(args: list) -> bytes:
    """`args`' output, refused with the program's name and its own error, never its arguments:
    what it wrote to stderr, or to stdout without any, as keytool does."""
    done = subprocess.run(args, capture_output=True)
    if done.returncode != 0:
        error = (done.stderr.strip() or done.stdout.strip()).decode(errors="replace")
        raise Refused(f"{Path(args[0]).name} failed: {error}")
    return done.stdout


def asset(version: str) -> str:
    """The APK's file name of the build of `version`."""
    return f"dsp-dispatch-relay-{version}.apk"


def digest(path: Path) -> str:
    """`path`'s SHA-256, in lowercase hex."""
    hashed = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1 << 20), b""):
            hashed.update(chunk)
    return hashed.hexdigest()


def following(released: Optional[str]) -> str:
    """The app version after `released`, its next patch, as the release command numbers it."""
    if released is None:
        return FIRST
    major, minor, patch = RELEASE.fullmatch(released).groups()
    return f"{major}.{minor}.{int(patch) + 1}"


def key(file: Path, env: dict, run: Run = run) -> str:
    """Writes the key to `file`, only its owner reading it; answers its certificate's SHA-256."""
    try:
        keystore = base64.b64decode("".join(env["RELAY_KEYSTORE"].split()), validate=True)
    except (KeyError, ValueError):
        raise Refused("RELAY_KEYSTORE is the signing key, a base64 PKCS12 keystore") from None
    with os.fdopen(os.open(file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb") as out:
        out.write(keystore)
    certificate = run([
        "keytool", "-exportcert", "-keystore", str(file), "-storetype", "PKCS12",
        "-storepass:env", "RELAY_KEYSTORE_PASSWORD", "-alias", env["RELAY_KEY_ALIAS"],
    ])
    return hashlib.sha256(certificate).hexdigest()


def latest(repository: str, run: Run = run) -> Optional[dict]:
    """GitHub's latest release of `repository`, none before any."""
    try:
        return json.loads(run(["gh", "api", f"repos/{repository}/releases/latest"]))
    except Refused as error:
        if "HTTP 404" in str(error):
            return None
        raise


def downloaded(repository: str, found: dict, most: int, run: Run = run) -> bytes:
    """The release asset `found`, refused over `most` bytes or unlike GitHub's own digest of it."""
    if not 0 < found["size"] <= most:
        raise Refused(f"{found['name']} on the latest release is over {most} bytes")
    data = run([
        "gh", "api", "-H", "Accept: application/octet-stream",
        f"repos/{repository}/releases/assets/{found['id']}",
    ])
    github = f"sha256:{hashlib.sha256(data).hexdigest()}"
    if len(data) != found["size"] or found.get("digest") != github:
        raise Refused(f"{found['name']} on the latest release differs from GitHub's digest of it")
    return data


def released(repository: str, run: Run = run) -> tuple:
    """The latest release's Production build, its entry and its release's assets by name: none
    before a release carried one."""
    release = latest(repository, run)
    if release is None:
        return None, {}
    assets = {found["name"]: found for found in release["assets"]}
    if RELEASED not in assets:
        return None, assets
    builds = json.loads(downloaded(repository, assets[RELEASED], RELEASED_MOST, run))
    entry = builds.get("production")
    # Its asset becomes a file's name here: only a release's own name is taken.
    if entry is not None and not (
        RELEASE.fullmatch(str(entry.get("version")))
        and entry.get("asset") == asset(entry["version"])
    ):
        raise Refused(f"{RELEASED} on {release['tag_name']} names no Production build of its own")
    return entry, assets


def version(build: str, wanted: Optional[str], repository: str, run: Run = run) -> dict:
    """The names of this commit's `build`, `wanted` naming Production's."""
    last, _ = released(repository, run)
    following_version = following(last and last["version"])
    code = int(run(["git", "rev-list", "--count", "HEAD"]).decode())
    if build == "dev":
        name = f"{following_version}-dev.{code}"
    elif wanted and RELEASE.fullmatch(wanted):
        name = wanted
    else:
        raise Refused("Production's version is three numbers, as 1.4.0")
    return {
        "commit": run(["git", "rev-parse", "HEAD"]).decode().strip(),
        "code": str(code),
        "name": name,
        "next": following_version,
        "changed": run(["git", "log", "-1", "--format=%H", "HEAD", "--", "android"]).decode().strip(),
    }


def entry(build: str, apk: Path, folder: Path, commit: str, code: int, name: str) -> dict:
    """Copies `apk` into `folder` as its asset and writes its entry beside it."""
    folder.mkdir(parents=True, exist_ok=True)
    made = {
        "version": name,
        "versionCode": code,
        "commit": commit,
        "asset": asset(name),
        "sha256": digest(apk),
        "size": apk.stat().st_size,
    }
    shutil.copyfile(apk, folder / made["asset"])
    (folder / f"{build}.json").write_text(json.dumps(made, indent=2) + "\n")
    return made


def tool(name: str, env: dict) -> str:
    """The newest Android build tool `name` in the SDK `ANDROID_HOME` names."""
    sdk = env.get("ANDROID_HOME") or env.get("ANDROID_SDK_ROOT")
    if not sdk:
        raise Refused("ANDROID_HOME names the Android SDK")
    versions = [
        found for found in (Path(sdk) / "build-tools").iterdir()
        if re.fullmatch(r"[0-9]+(\.[0-9]+)*", found.name) and (found / name).exists()
    ]
    if not versions:
        raise Refused(f"No build tools with {name} in {sdk}")
    newest = max(versions, key=lambda found: [int(n) for n in found.name.split(".")])
    return str(newest / name)


def signers(text: str) -> list:
    """Each certificate's SHA-256, once, of those `apksigner verify --print-certs-pem` prints: the
    certificates themselves, as each version of the tool labels its signers its own way."""
    return sorted({
        hashlib.sha256(base64.b64decode("".join(pem.split()))).hexdigest()
        for pem in PEM.findall(text)
    })


def badging(text: str) -> dict:
    """The app ID, versionCode and versionName `aapt2 dump badging` names."""
    found = re.search(
        r"^package: name='([^']*)' versionCode='([^']*)' versionName='([^']*)'", text, re.M
    )
    if not found:
        raise Refused("aapt2 named no package")
    return {"package": found[1], "versionCode": found[2], "versionName": found[3]}


def signed(folder: Path, build: str, certificate: str, env: dict, run: Run = run) -> dict:
    """The entry of `build` in `folder`, refused unless its APK is the one it names, the build's
    app of its version, signed by `certificate` alone."""
    made = json.loads((folder / f"{build}.json").read_text())
    apk = folder / made["asset"]
    if apk.is_symlink() or apk.stat().st_size != made["size"] or digest(apk) != made["sha256"]:
        raise Refused(f"{made['asset']} differs from {build}.json")
    certificates = signers(
        run([tool("apksigner", env), "verify", "--print-certs-pem", str(apk)]).decode()
    )
    if certificates != [certificate]:
        raise Refused(f"{made['asset']} is not signed by {build}'s key alone")
    app = badging(run([tool("aapt2", env), "dump", "badging", str(apk)]).decode())
    expected = {
        "package": PACKAGES[build],
        "versionCode": str(made["versionCode"]),
        "versionName": made["version"],
    }
    if app != expected:
        raise Refused(f"{made['asset']} is {app}, not {expected}")
    return made


def carry(folder: Path, repository: str, run: Run = run) -> Optional[dict]:
    """The latest release's Production build, its entry and APK, in `folder`; none before any."""
    made, assets = released(repository, run)
    if made is None:
        return None
    if made["asset"] not in assets:
        raise Refused(f"The latest release lacks {made['asset']}")
    data = downloaded(repository, assets[made["asset"]], APK_MOST, run)
    if len(data) != made.get("size") or hashlib.sha256(data).hexdigest() != made.get("sha256"):
        raise Refused(f"{made['asset']} on the latest release differs from its {RELEASED}")
    folder.mkdir(parents=True, exist_ok=True)
    (folder / made["asset"]).write_bytes(data)
    (folder / "production.json").write_text(json.dumps(made, indent=2) + "\n")
    return made


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("key").add_argument("file", type=Path)
    named = commands.add_parser("version")
    named.add_argument("build", choices=PACKAGES)
    named.add_argument("wanted", nargs="?")
    made = commands.add_parser("entry")
    made.add_argument("build", choices=PACKAGES)
    made.add_argument("apk", type=Path)
    made.add_argument("folder", type=Path)
    made.add_argument("commit")
    made.add_argument("code", type=int)
    made.add_argument("name")
    checked = commands.add_parser("signed")
    checked.add_argument("folder", type=Path)
    checked.add_argument("build", choices=PACKAGES)
    checked.add_argument("cert")
    commands.add_parser("carry").add_argument("folder", type=Path)
    args = parser.parse_args(argv)
    repository = os.environ.get("GITHUB_REPOSITORY", "dispatch-systems/dispatch-platform")
    try:
        if args.command == "key":
            print(f"cert={key(args.file, os.environ)}")
        elif args.command == "version":
            for name, value in version(args.build, args.wanted, repository).items():
                print(f"{name}={value}")
        elif args.command == "entry":
            entry(args.build, args.apk, args.folder, args.commit, args.code, args.name)
        elif args.command == "signed":
            made = signed(args.folder, args.build, args.cert, os.environ)
            print(f"Signed {args.build}'s {made['version']}, versionCode {made['versionCode']}")
        else:
            made = carry(args.folder, repository)
            print(f"Carried Production's {made['version']}" if made else "No Production app yet")
    except Refused as error:
        sys.exit(f"relay: {error}")


if __name__ == "__main__":
    main()
