"""Runs only inside the Dev sandbox. Keep backend/config output away from release logs."""
import json
import os
from pathlib import Path
import resource
import sqlite3
import subprocess
import sys

resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_FSIZE, (1024 ** 3, 1024 ** 3))
resource.setrlimit(resource.RLIMIT_NOFILE, (256, 256))
resource.setrlimit(resource.RLIMIT_AS, (4 * 1024 ** 3, 4 * 1024 ** 3))
BACKEND = "/runtime/services/rust/dispatch-backend"


def run(args, env=None):
    # Files live only on the bounded disposable mount, including on a nonzero exit.
    with open("/work/stdout", "w+b") as out, open("/work/stderr", "w+b") as err:
        result = subprocess.run(args, env=env, stdout=out, stderr=err, check=False)
        if result.returncode:
            raise ValueError("backend exit failure")
        if out.tell() > 1024 * 1024:
            raise ValueError("oversized backend report")
        out.seek(0)
        return json.load(out)


def restore(target):
    return run([BACKEND, "restore", "/input", target], {"PATH": "/usr/bin:/bin"})


def main():
    os.umask(0o077)
    restored = restore("/work/restore")
    databases = sorted(Path("/work/restore").rglob("*.sqlite"))
    integrity = True
    for database in databases:
        try:
            with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as db:
                checked = db.execute("PRAGMA integrity_check").fetchall() == [("ok",)]
                integrity = checked and integrity
        except sqlite3.DatabaseError:
            integrity = False
    answer = {"files": restored["files"], "databases": len(databases), "integrity": integrity}
    if sys.argv[1] == "upgrade" and integrity and databases:
        restore("/work/upgrade")
        report = run(["/bin/bash", "--noprofile", "--norc", "-c", r'''
set -eu
set -a
. /input/config/platform.env
set +a
test "${DISPATCH_ENVIRONMENT:-}" = production
test "${NODE_ENV:-production}" = production
test "${DISPATCH_PROVIDER_MODE:-}" = native
export DISPATCH_STATE_ROOT=/work/upgrade DISPATCH_ARTIFACT_ROOT=/runtime
cd /runtime
exec /runtime/services/rust/dispatch-backend upgrade-check /work/upgrade
'''], {"PATH": "/usr/bin:/bin"})
        if (not isinstance(report["carryOverFailures"], list)
                or not isinstance(report["errors"], list)
                or not isinstance(report["migrations"], dict)
                or not all(type(value) is int and value >= 0 for value in report["migrations"].values())
                or type(report["databases"]) is not int or report["databases"] < 0
                or report["stopped"] is not None and not isinstance(report["stopped"], str)):
            raise ValueError("invalid startup report")
        # Never export arbitrary fields from private data or credentials.
        answer["upgrade"] = {key: report[key] for key in ("databases", "migrations", "stopped")}
        answer["upgrade"]["carryOverFailures"] = [True for _ in report["carryOverFailures"]]
        answer["upgrade"]["errors"] = [True for _ in report["errors"]]
    print(json.dumps(answer))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit(1)
