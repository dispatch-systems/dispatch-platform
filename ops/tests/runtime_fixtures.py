"""Synthetic runtime contents and private temporary setup for launcher tests."""
import hashlib
import json
from pathlib import Path

TOOLING = Path(__file__).parents[2] / "ops/launchers"


def private_directory(directory):
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    return directory


def write_json(filename, value):
    private_directory(filename.parent)
    filename.write_text(json.dumps(value))
    filename.chmod(0o600)


def artifact(root, commit):
    root.mkdir(mode=0o700, parents=True)
    contents = {
        "services/rust/dispatch-backend": "#!/bin/sh\ntouch " + str(root.parent / "executed"),
        "dashboard/index.html": "<h1>Dispatch</h1>",
        "tooling/build-info.json": json.dumps({"commit": commit, "hostManagement": 1}),
    }
    for name, text in contents.items():
        target = root / name
        target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        target.write_text(text)
    files = []
    for item in sorted(root.rglob("*")):
        if item.is_file():
            files.append({"path": item.relative_to(root).as_posix(),
                          "sha256": hashlib.sha256(item.read_bytes()).hexdigest(),
                          "size": item.stat().st_size})
    manifest = {"format": 3, "version": "0.1.0-dev.0", "runtime": "rust", "schema": 3, "files": files}
    manifest["digest"] = hashlib.sha256(json.dumps(manifest, separators=(",", ":")).encode()).hexdigest()
    (root / "release.json").write_text(json.dumps(manifest))
    return manifest
