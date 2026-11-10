#!/usr/bin/env python3
"""Runs cargo-nextest, the release `nextest.json` pins, with the arguments given, as
`nextest.py run --locked`.

The core jobs' Rust tests run through it: `cargo test` runs one test program after another,
each of its tests on a thread, while nextest runs every test of every program side by side, each
in its own process. The archive downloads once into `.ci-tools/nextest/`, and its SHA-256 is
checked against the pin there and on every run, before the executable is unpacked from it.
"""
from __future__ import annotations

import hashlib
import json
import os
import platform
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PIN = Path(__file__).with_suffix(".json")
NAME = "cargo-nextest"
# The archive is about 12 MB; anything much larger isn't it.
LIMIT = 64 * 1024 * 1024


def target() -> str:
    """This machine's key in the pin's archives, as linux_x64."""
    machine = {"x86_64": "x64", "amd64": "x64", "aarch64": "arm64", "arm64": "arm64"}.get(
        platform.machine().lower(), platform.machine().lower()
    )
    return f"{platform.system().lower()}_{machine}"


def triple(key: str) -> str:
    """The release's name for the platform `key` names."""
    return {
        "linux_x64": "x86_64-unknown-linux-gnu",
        "linux_arm64": "aarch64-unknown-linux-gnu",
    }[key]


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive(pin: dict, key: str, folder: Path) -> bytes:
    """The pinned archive's bytes, downloaded into `folder` the first time; either way, checked
    against the pin."""
    expected = pin["archives"].get(key)
    if not expected:
        raise SystemExit(f"cargo-nextest has no pinned archive for {key}")
    name = f"{NAME}-{pin['version']}-{triple(key)}.tar.gz"
    path = folder / name
    if not path.exists():
        url = f"https://github.com/nextest-rs/nextest/releases/download/{NAME}-{pin['version']}/{name}"
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url, timeout=60) as response:
                    data = response.read(LIMIT + 1)
                break
            except urllib.error.HTTPError as error:
                retry = error.code in (502, 504) and attempt < 2
                error.close()
                if not retry:
                    raise
        if len(data) > LIMIT or digest(data) != expected:
            raise SystemExit("cargo-nextest archive checksum mismatch")
        folder.mkdir(parents=True, exist_ok=True)
        # Written whole under another name first, so an interrupted download leaves no archive.
        with tempfile.NamedTemporaryFile(dir=folder, delete=False) as staged:
            staged.write(data)
        os.replace(staged.name, path)
    data = path.read_bytes()
    if digest(data) != expected:
        raise SystemExit("Cached cargo-nextest archive checksum mismatch")
    return data


def unpack(data: bytes, folder: Path) -> Path:
    """The archive's one executable, written to `folder`: whole under another name, then renamed
    over the last one, so a run already executing that one keeps it, and two runs starting
    together each execute a complete file."""
    with tempfile.TemporaryFile() as staged:
        staged.write(data)
        staged.seek(0)
        with tarfile.open(fileobj=staged) as package:
            member = package.getmember(NAME)
            if not member.isfile():
                raise SystemExit("cargo-nextest archive has no regular executable")
            contents = package.extractfile(member).read()
    with tempfile.NamedTemporaryFile(dir=folder, delete=False) as written:
        written.write(contents)
    os.chmod(written.name, 0o700)
    executable = folder / NAME
    os.replace(written.name, executable)
    return executable


def main(args: list[str]) -> None:
    pin = json.loads(PIN.read_text())
    folder = ROOT / ".ci-tools/nextest" / pin["version"]
    executable = unpack(archive(pin, target(), folder), folder)
    # cargo-nextest called directly, not as a cargo subcommand, takes `nextest` first.
    os.execv(executable, [str(executable), "nextest", *args])


if __name__ == "__main__":
    main(sys.argv[1:])
