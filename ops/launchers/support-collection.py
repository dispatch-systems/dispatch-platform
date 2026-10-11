#!/usr/bin/env python3
"""Verify an assigned collector with a configured Dev account, outside Hermes."""
import argparse
import asyncio
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time


PINNED_BROWSER = json.loads(Path(__file__).with_name("browseros-release.json").read_text())
BROWSER = Path("/opt/dispatch-browseros") / PINNED_BROWSER["version"] / "browseros"
HOST_NAMESPACES = ("pid", "net", "mnt", "ipc", "uts", "user")
SANDBOXES = (Path("/usr/local/libexec/dispatch-dev/bwrap"), Path("/usr/bin/bwrap"))


def file_digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def root_owned(path, executable=False):
    """Run on the host, before ownership is remapped by the collector namespace."""
    for entry in (path, *path.parents):
        info = entry.lstat()
        expected = stat.S_ISREG(info.st_mode) if entry == path else stat.S_ISDIR(info.st_mode)
        if (not expected or info.st_uid != 0 or info.st_mode & 0o022
                or (entry == path and executable and not info.st_mode & 0o111)):
            raise ValueError("Trusted installed browser runtime required")


def trusted_sandbox():
    # The dedicated Dev binary has the host's nested-userns policy. Never accept
    # an agent-selected executable or validate host ownership after UID remapping.
    path = SANDBOXES[0]
    if not path.exists() and not path.is_symlink():
        path = SANDBOXES[1]
    root_owned(path, executable=True)
    return path


def browser_authority(binary, source_sha, sandbox_executable):
    root_owned(BROWSER, executable=True)
    directory = BROWSER.parent
    manifest_path = directory / "dispatch-install.json"
    root_owned(manifest_path)
    manifest = json.loads(manifest_path.read_text())
    if (set(manifest) != set(PINNED_BROWSER) | {"files"}
            or any(manifest[key] != value for key, value in PINNED_BROWSER.items())
            or not isinstance(manifest["files"], list)):
        raise ValueError("Pinned installed browser release required")
    inventory = {}
    for entry in manifest["files"]:
        if not isinstance(entry, dict) or set(entry) != {"path", "sha256"}:
            raise ValueError("Invalid installed browser inventory")
        relative = entry["path"]
        if (not isinstance(relative, str) or Path(relative).is_absolute()
                or any(part in ("", ".", "..") for part in relative.split("/"))
                or relative in inventory or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])):
            raise ValueError("Invalid installed browser inventory")
        inventory[relative] = entry["sha256"]
    actual = set()
    for path in directory.rglob("*"):
        info = path.lstat()
        if (info.st_uid != 0 or info.st_mode & 0o022
                or not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode))):
            raise ValueError("Trusted installed browser runtime required")
        if path.is_file():
            relative = path.relative_to(directory).as_posix()
            actual.add(relative)
            if path != manifest_path and inventory.get(relative) != file_digest(path):
                raise ValueError("Installed browser file changed")
    if actual != set(inventory) | {"dispatch-install.json"} or "browseros" not in inventory:
        raise ValueError("Installed browser inventory differs")
    tools = {"browser": BROWSER, "sandbox": sandbox_executable, "display": Path("/usr/bin/Xvfb")}
    for path in tools.values():
        root_owned(path, executable=True)
    return {"schemaVersion": 1, "sourceSha": source_sha, "backendSha256": file_digest(binary),
            "hostNamespaces": {name: os.readlink(f"/proc/self/ns/{name}") for name in HOST_NAMESPACES},
            "executables": {name: {"path": str(path), "sha256": file_digest(path)} for name, path in tools.items()}}


def seal_browser_authority(egress, authority):
    # The whole directory is mounted read-only in the collector. Only the host broker
    # retains write access; no branch/agent process receives a writable descriptor.
    path = egress / "browser-authority.json"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o400)
    with os.fdopen(descriptor, "w") as stream:
        json.dump(authority, stream, separators=(",", ":"))
        stream.flush()
        os.fsync(stream.fileno())


def private_file(path):
    metadata = path.lstat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()
            or metadata.st_mode & 0o077 or metadata.st_nlink != 1):
        raise ValueError("Private credential reference required")


def private_state_file(state, relative):
    path = state
    for component in relative.parts[:-1]:
        path /= component
        metadata = path.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
            raise ValueError("Private isolated state directory required")
    path /= relative.name
    private_file(path)
    return path


def revision(source, dsp, provider):
    accounts = source / "data/platform/accounts.sqlite"
    private_file(accounts)
    with sqlite3.connect(f"file:{accounts}?mode=ro", uri=True) as connection:
        active = connection.execute("SELECT 1 FROM dsps WHERE id=? AND status='active'", (dsp,)).fetchone()
    if not active:
        return None
    path = source / "dsps" / dsp / "data" / provider / f"{provider}.sqlite"
    private_file(path)
    with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as connection:
        return connection.execute("SELECT enabled,revision FROM connections WHERE provider=?", (provider,)).fetchone()


def project_account(source, dsp, provider, broker):
    """Only selected authority metadata and vault material cross the host boundary."""
    accounts = source / "data/platform/accounts.sqlite"
    private_file(accounts)
    with sqlite3.connect(f"file:{accounts}?mode=ro", uri=True) as connection:
        active = connection.execute("SELECT name,timezone FROM dsps WHERE id=? AND status='active'", (dsp,)).fetchone()
    if not active:
        raise ValueError("Configured Dev verification account unavailable")
    current = revision(source, dsp, provider)
    if not current or current[0] != 1:
        raise ValueError("Configured Dev provider is disabled")
    dsp_database = source / "dsps" / dsp / "data/dispatch.sqlite"
    private_file(dsp_database)
    with sqlite3.connect(f"file:{dsp_database}?mode=ro", uri=True) as connection:
        setting = connection.execute("SELECT value FROM settings WHERE key='dsp.profile'").fetchone()
    profile = json.loads(setting[0]) if setting else {}
    selected_profile = {key:profile.get(key, "") for key in ("abbreviation", "stationCode")}
    platform = broker / "data/platform"
    platform.mkdir(parents=True)
    with sqlite3.connect(platform / "accounts.sqlite") as connection:
        connection.execute("CREATE TABLE dsps(id TEXT PRIMARY KEY,status TEXT,name TEXT,timezone TEXT)")
        connection.execute("INSERT INTO dsps VALUES(?, 'active', ?, ?)", (dsp, *active))
    area = broker / "dsps" / dsp
    area.joinpath("data").mkdir(parents=True)
    with sqlite3.connect(area / "data/dispatch.sqlite") as connection:
        connection.execute("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT)")
        connection.execute("INSERT INTO settings VALUES('dsp.profile', ?)", (json.dumps(selected_profile),))
    connection_root = area / "data" / provider
    connection_root.mkdir(parents=True)
    with sqlite3.connect(connection_root / f"{provider}.sqlite") as connection:
        connection.execute("CREATE TABLE connections(provider TEXT PRIMARY KEY,enabled INTEGER,revision INTEGER)")
        connection.execute("INSERT INTO connections VALUES(?,?,?)", (provider, *current))
    secrets = area / "secrets"
    secrets.mkdir()
    for name in ("vault.key", f"{provider}.enc"):
        original = source / "dsps" / dsp / "secrets" / name
        private_file(original)
        shutil.copyfile(original, secrets / name)
    return current


def sandbox(artifact, state, env, broker=None, egress=None, target_dsp=None):
    """No host account homes, live DSP roots, keys or network namespace are mounted."""
    executable = Path(env["DISPATCH_BWRAP_EXECUTABLE"])
    if executable not in SANDBOXES:
        raise ValueError("Trusted installed sandbox required")
    command = [str(executable), "--unshare-all", "--die-with-parent", "--new-session", "--cap-drop", "ALL",
               "--clearenv", "--ro-bind", "/usr", "/usr", "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
               "--dir", "/sandbox", "--symlink", "usr/bin", "/bin", "--symlink", "usr/lib", "/lib"]
    if Path("/usr/lib64").exists():
        command.extend(["--symlink", "usr/lib64", "/lib64"])
    for path in ("/etc/ssl", "/etc/fonts", "/etc/passwd", "/etc/group", "/etc/nsswitch.conf"):
        if Path(path).exists():
            command.extend(["--ro-bind", path, path])
    command.extend(["--ro-bind", str(artifact), str(artifact), "--bind", str(state), str(state)])
    if broker:
        browser = Path(env["DISPATCH_BROWSEROS_EXECUTABLE"]).resolve(strict=True)
        if not browser.is_relative_to("/opt/dispatch-browseros"):
            raise ValueError("Installed provider browser required")
        command.extend(["--ro-bind", str(browser.parent), str(browser.parent)])
        command.extend(["--bind", str(broker), "/broker/dev", "--ro-bind", str(egress), "/run/support-provider",
                        "--ro-bind", str(Path(__file__).resolve()), "/runtime/support-collection.py",
                        "--ro-bind", str(Path(__file__).with_name("browseros-release.json").resolve()),
                        "/runtime/browseros-release.json"])
        # Credentials created by the changed collector vanish even after SIGKILL.
        command.extend(["--perms", "0700", "--tmpfs", str(state / "dsps" / target_dsp / "secrets")])
    env.update({"HOME":"/sandbox", "PATH":"/usr/bin:/bin", "TMPDIR":"/tmp"})
    for name, value in env.items():
        command.extend(["--setenv", name, value])
    command.extend(["--chdir", str(artifact)])
    return command


def die_with_parent(parent_pid=os.getpid()):
    if ctypes.CDLL(None).prctl(1, signal.SIGKILL, 0, 0, 0) != 0:
        os._exit(1)
    if os.getppid() != parent_pid:
        os._exit(1)


async def sandbox_child(binary):
    """Loopback HTTP clients and provider browser sockets share the host policy broker."""
    async def relay(reader, writer):
        peer = None
        try:
            upstream, peer = await asyncio.open_unix_connection("/run/support-provider/egress.sock")
            async def copy(source, destination):
                while data := await source.read(65536):
                    destination.write(data)
                    await destination.drain()
                destination.close()
            await asyncio.gather(copy(reader, peer), copy(upstream, writer))
        except (OSError, ConnectionError):
            pass
        finally:
            writer.close()
            if peer:
                peer.close()
    server = await asyncio.start_server(relay, "127.0.0.1", 17892)
    async with server:
        process = await asyncio.create_subprocess_exec(binary, "support-verify-collection")
        return await process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worktree", type=Path, required=True)
    parser.add_argument("--ticket", required=True)
    parser.add_argument("--provider", choices=("paycom", "cortex"), required=True)
    parser.add_argument("--request", required=True)
    parser.add_argument("--state-root", type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    workspace = Path(os.environ.get("SUPPORT_WORKSPACE_ROOT", str(Path.home() / "dispatch-platform"))).resolve(strict=True)
    worktree = args.worktree.resolve(strict=True)
    if worktree.parent != workspace / "worktrees" or not re.fullmatch(r"support_[0-9a-f]{32}", args.ticket):
        raise ValueError("Assigned support worktree and ticket required")
    branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=worktree, text=True).strip()
    ticket_hex = args.ticket.removeprefix("support_")
    if branch not in (f"support-dev-{ticket_hex}", f"support-production-{ticket_hex}"):
        raise ValueError("Assigned support branch required")
    reference_path = Path(os.environ["SUPPORT_COLLECTION_REFERENCE_FILE"])
    private_file(reference_path)
    reference = json.loads(reference_path.read_text())
    if set(reference) != {"sourceRoot", "sourceDsp"}:
        raise ValueError("Invalid credential reference")
    source = Path(reference["sourceRoot"]).resolve(strict=True)
    dsp = reference["sourceDsp"]
    if source != workspace / "dev" or not re.fullmatch(r"dsp_[0-9a-f]{32}", dsp):
        raise ValueError("Configured Dev verification account required")
    request = json.loads(args.request)
    if not isinstance(request, dict) or set(request) - {"date", "week", "collection"}:
        raise ValueError("Read-only collection request required")
    artifact = (worktree / ".build").resolve(strict=True)
    if artifact.parent != worktree or artifact.name != ".build":
        raise ValueError("Assigned branch artifact must be a real directory")
    binary = artifact / "services/rust/dispatch-backend"
    metadata = json.loads((artifact / "tooling/build-info.json").read_text())
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=worktree, text=True).strip()
    if metadata["commit"] != head:
        raise ValueError("Build the current assigned branch before collection verification")
    scratch = Path("/tmp") / f"dispatch-{worktree.name}"
    scratch.mkdir(mode=0o700, parents=True, exist_ok=True)
    metadata = scratch.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise ValueError("Private assigned scratch directory required")
    if args.state_root:
        state = args.state_root.resolve(strict=True)
        if state.parent != scratch or not state.name.startswith("support-native-"):
            raise ValueError("Assigned private native verification state required")
        metadata = state.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
            raise ValueError("Private verification state required")
        marker = state / ".support-verification.json"
        private_file(marker)
        if json.loads(marker.read_text()) != {"ticketId":args.ticket,"sourceSha":head}:
            raise ValueError("Verification state belongs to another ticket or commit")
    else:
        state = Path(tempfile.mkdtemp(prefix=f"support-native-{args.provider}-", dir=scratch))
        (state / ".support-verification.json").write_text(json.dumps({"ticketId":args.ticket,"sourceSha":head}))
    sandbox_executable = trusted_sandbox()
    env = {"NODE_ENV":"development", "DISPATCH_ENVIRONMENT":"preview", "DISPATCH_STATE_ROOT":str(state),
           "DISPATCH_ARTIFACT_ROOT":str(artifact), "DISPATCH_ORIGIN":"http://localhost:5173",
           "DISPATCH_PROVIDER_MODE":"fixture", "DISPATCH_DEV_MAIL_MODE":"capture",
           "DISPATCH_BROWSEROS_EXECUTABLE":str(BROWSER), "DISPATCH_BWRAP_EXECUTABLE":str(sandbox_executable)}
    if not args.state_root:
        subprocess.run(sandbox(artifact, state, dict(env)) + [str(binary), "seed"], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    accounts = private_state_file(state, Path("data/platform/accounts.sqlite"))
    with sqlite3.connect(f"file:{accounts}?mode=ro", uri=True) as connection:
        row = connection.execute("SELECT id FROM dsps WHERE code='nll' AND status='active'").fetchone()
    if row is None or not isinstance(row[0], str) or not re.fullmatch(r"dsp_[0-9a-f]{32}", row[0]):
        raise ValueError("Isolated verification DSP unavailable")
    authority = browser_authority(binary, head, sandbox_executable)
    lock_root = source / "dsps" / dsp / "state/credential-leases"
    lock_root.mkdir(mode=0o700, parents=True, exist_ok=True)
    lock_path = lock_root / f"{args.provider}.lock"
    lease = os.open(lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    broker_process = None
    process = None
    try:
        private_file(lock_path)
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # Vault material stays on RAM-backed storage rather than persistent scratch.
        with tempfile.TemporaryDirectory(prefix="support-broker-", dir="/dev/shm") as temporary:
            broker_root = Path(temporary)
            projection = broker_root / "dev"
            projection.mkdir()
            current = project_account(source, dsp, args.provider, projection)
            egress = broker_root / "network"
            egress.mkdir()
            seal_browser_authority(egress, authority)
            installed_artifact = (source / ".build").resolve(strict=True)
            installed = installed_artifact / "services/rust/dispatch-backend"
            broker_env = {**env, "DISPATCH_ARTIFACT_ROOT":str(installed_artifact)}
            broker_process = subprocess.Popen([str(installed), "support-provider-egress", args.provider, str(egress)],
                                              cwd=installed_artifact, env=broker_env, stdout=subprocess.DEVNULL,
                                              stderr=subprocess.DEVNULL, start_new_session=True, preexec_fn=die_with_parent)
            for _ in range(100):
                if (egress / "egress.sock").exists():
                    break
                if broker_process.poll() is not None:
                    raise ValueError("Installed provider egress unavailable")
                time.sleep(0.05)
            if not (egress / "egress.sock").exists():
                raise ValueError("Installed provider egress unavailable")
            env.update({"DISPATCH_PROVIDER_MODE":"native",
                        "SUPPORT_VERIFICATION_EGRESS":"/run/support-provider/egress.sock"})
            payload = {"sourceRoot":"/broker/dev", "sourceDsp":dsp, "targetDsp":row[0], "provider":args.provider,
                       "request":request, "ticketId":args.ticket}
            revoked = False
            with tempfile.TemporaryFile(mode="w+") as output:
                command = sandbox(artifact, state, dict(env), projection, egress, row[0])
                command.extend(["/usr/bin/python3", "/runtime/support-collection.py", "--sandbox-child", str(binary)])
                process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=output, stderr=subprocess.DEVNULL,
                                           text=True, start_new_session=True)
                process.stdin.write(json.dumps(payload))
                process.stdin.close()
                while process.poll() is None:
                    if revision(source, dsp, args.provider) != current:
                        revoked = True
                        process.kill()
                        break
                    time.sleep(0.2)
                result = process.wait()
                revoked = revoked or revision(source, dsp, args.provider) != current
                output.seek(0, os.SEEK_END)
                output.seek(max(0, output.tell() - 16384))
                raw = output.read()
            if revoked:
                outcome = {"status":"reference_changed", "passed":False, "error":"connection_changed"}
            elif result:
                outcome = {"status":"verification_failed", "passed":False, "error":"collector_verification_failed"}
            else:
                parsed = json.loads(raw.strip().splitlines()[-1])
                status = parsed.get("status")
                if status not in {"succeeded", "failed", "queued", "cancelled", "waiting_verification", "reference_changed"}:
                    status = "verification_failed"
                error = parsed.get("error")
                if error is not None and (not isinstance(error, str) or not re.fullmatch(r"[a-z_]{1,64}", error)):
                    error = "collector_verification_failed"
                counts = parsed.get("counts") if isinstance(parsed.get("counts"), dict) else {}
                counts = {key:value for key in ("employees", "timecards", "itineraries", "rows")
                          if isinstance(value := counts.get(key), int) and not isinstance(value, bool) and 0 <= value <= 10_000_000}
                outcome = {"provider":args.provider, "status":status,
                           "passed":parsed.get("passed") is True and status == "succeeded", "error":error, "counts":counts}
    finally:
        for child in (process, broker_process):
            if child and child.poll() is None:
                child.kill()
                child.wait()
        os.close(lease)
    # Private state is for the trusted browser supervisor, never an agent tool.
    outcome.update({"stateRoot":str(state), "targetDsp":row[0], "sourceSha":head})
    print(json.dumps(outcome))


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--sandbox-child":
        raise SystemExit(asyncio.run(sandbox_child(sys.argv[2])))
    try:
        main()
    except (OSError, ValueError, KeyError, IndexError, sqlite3.Error, subprocess.CalledProcessError, json.JSONDecodeError):
        print(json.dumps({"status":"unavailable", "passed":False, "error":"verification_reference_unavailable"}))
        raise SystemExit(1)
