#!/usr/bin/python3
"""Synthetic backend boundary for host tests; filesystem and SQLite work is real."""
import json
import mmap
import os
from pathlib import Path
import shutil
import sys

control = json.loads(Path("/input/control.json").read_text())
command, *args = sys.argv[1:]
if command == "restore":
    source, target = map(Path, args)
    assert not target.exists()
    assert not Path("/home").exists()
    assert not Path("/root").exists()
    assert sorted(Path("/sys/class/net").glob("*")) == []
    # No network request is needed to prove that the host's routes are absent.
    assert len(Path("/proc/net/route").read_text().splitlines()) == 1
    assert os.stat("/work").st_dev == os.stat("/input").st_dev
    if target.name == "upgrade":
        assert not Path("/work/restore").exists()
    assert os.statvfs("/tmp").f_blocks * os.statvfs("/tmp").f_frsize <= 64 * 1024 ** 2
    if control.get("largeFile"):
        with open("/work/above-old-limit", "wb") as output:
            output.truncate(1024 ** 3 + 1)
        Path("/work/above-old-limit").unlink()
    if control.get("fileLimit"):
        with open("/work/oversize", "wb") as output:
            output.truncate(8 * 1024 ** 3 + 1)
    if control.get("memoryLimit"):
        # Reserve address space without touching/allocating physical pages.
        with mmap.mmap(-1, 4 * 1024 ** 3 + 1):
            pass
    if control.get("descriptorLimit"):
        descriptors = [open("/dev/null", "rb") for _ in range(512)]
        for descriptor in descriptors:
            descriptor.close()
    target.mkdir(mode=0o700)
    manifest = json.loads((source / "backup.json").read_text())
    for entry in manifest["files"]:
        output = target / entry["path"]
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / entry["path"], output)
        output.chmod(0o600)
    if not control["restorable"]:
        next(target.rglob("*.sqlite")).write_text("corrupt database")
    if control.get("noDatabases"):
        for path in target.rglob("*.sqlite"):
            path.unlink()
    print(json.dumps({"target": str(target), "files": control.get("files", len(manifest["files"]))}))
elif command == "upgrade-check":
    assert args == ["/work/upgrade"]
    assert os.environ["DISPATCH_STATE_ROOT"] == "/work/upgrade"
    assert os.environ["DISPATCH_ARTIFACT_ROOT"] == "/runtime"
    assert "HOME" not in os.environ
    report = control["report"]
    print(json.dumps(report))
    sys.exit(1 if report["stopped"] is not None and not control.get("successExit") else 0)
else:
    raise ValueError(command)
