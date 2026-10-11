"""Trusted supervisor: changed builds and servers never execute with host credentials.

Builds have no network. A preview's only network path is a private Unix socket, forwarded
by this supervisor to a loopback browser origin. Git metadata is projected, never writable
host administration. This file must come from the installed worker, not the ticket branch.
"""
import argparse
import json
import os
from pathlib import Path
import selectors
import shutil
import socket
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def relay(left, right):
    with selectors.DefaultSelector() as selector:
        selector.register(left, selectors.EVENT_READ, right)
        selector.register(right, selectors.EVENT_READ, left)
        while True:
            for event, _ in selector.select():
                data = event.fileobj.recv(65536)
                if not data:
                    return
                event.data.sendall(data)


class UnixServer(socketserver.ThreadingUnixStreamServer):
    daemon_threads = True


class TcpServer(socketserver.ThreadingTCPServer):
    daemon_threads = True
    allow_reuse_address = True


def inner_preview(input):
    # Executed only after bwrap has removed host networking and all private host paths.
    state = Path("/state")
    environment = {
        "PATH": "/usr/bin:/bin", "HOME": "/tmp", "NODE_ENV": "development",
        "DISPATCH_ENVIRONMENT": "preview", "DISPATCH_STATE_ROOT": str(state),
        "DISPATCH_ARTIFACT_ROOT": "/artifact", "DISPATCH_PROVIDER_MODE": "fixture",
        "DISPATCH_DEV_MAIL_MODE": "capture", "DISPATCH_STANDALONE": "1",
        "DISPATCH_ORIGIN": input["origin"], "PORT": "39000",
    }
    backend = "/artifact/services/rust/dispatch-backend"
    if not (state / "data/platform/accounts.sqlite").exists():
        subprocess.run([backend, "seed"], env=environment, check=True, stdout=subprocess.DEVNULL)
    child = subprocess.Popen([backend, "serve"], env=environment, stdout=subprocess.DEVNULL)

    class Handler(socketserver.BaseRequestHandler):
        def handle(self):
            try:
                with socket.create_connection(("127.0.0.1", 39000), 15) as upstream:
                    relay(self.request, upstream)
            except (OSError, TimeoutError):
                pass

    address = "/bridge/application.sock"
    try:
        os.unlink(address)
    except FileNotFoundError:
        pass
    server = UnixServer(address, Handler)
    os.chmod(address, 0o600)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return child.wait()


def runtime():
    args = ["bwrap", "--unshare-all", "--die-with-parent", "--new-session", "--cap-drop", "ALL",
            "--clearenv", "--ro-bind", "/usr", "/usr", "--ro-bind", "/bin", "/bin",
            "--ro-bind", "/lib", "/lib", "--proc", "/proc", "--dev", "/dev",
            "--tmpfs", "/tmp", "--dir", "/sandbox", "--dir", "/sandbox/home",
            "--setenv", "HOME", "/sandbox/home",
            "--setenv", "PATH", "/sandbox/bin:/sandbox/.cargo/bin:/usr/bin:/bin",
            "--setenv", "TMPDIR", "/tmp"]
    if Path("/lib64").exists():
        args += ["--ro-bind", "/lib64", "/lib64"]
    for item in ("/etc/alternatives", "/etc/ld.so.cache"):
        if Path(item).exists():
            args += ["--ro-bind", item, item]
    return args


def project_git(root, destination):
    destination.mkdir(parents=True, exist_ok=True, mode=0o700)
    common = Path(git(root, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    index = Path(git(root, "rev-parse", "--path-format=absolute", "--git-path", "index"))
    workspace = root.parent.parent
    target = workspace / "dev/.git"
    branch = git(root, "symbolic-ref", "--short", "HEAD")
    if any(c not in "abcdefghijklmnopqrstuvwxyz0123456789-" for c in branch):
        raise ValueError("Unsupported assigned branch")
    (destination / "refs/heads").mkdir(parents=True, exist_ok=True)
    (destination / "refs/remotes/origin").mkdir(parents=True, exist_ok=True)
    (destination / "HEAD").write_text(f"ref: refs/heads/{branch}\n")
    (destination / "refs/heads" / branch).write_text(git(root, "rev-parse", "HEAD") + "\n")
    (destination / "refs/remotes/origin/main").write_text(git(root, "rev-parse", "origin/main") + "\n")
    (destination / "config").write_text(f"[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tworktree = {root}\n")
    shutil.copyfile(index, destination / "index")
    pointer = destination.parent / "git-pointer"
    pointer.write_text(f"gitdir: {target}\n")
    return ["--bind", str(destination), str(target), "--ro-bind", str(common / "objects"),
            str(target / "objects"), "--ro-bind", str(pointer), str(root / ".git")]


def build(root, node, command):
    scratch = Path(f"/tmp/dispatch-{root.name}")
    scratch.mkdir(parents=True, exist_ok=True, mode=0o700)
    home = Path.home()
    args = runtime() + ["--bind", str(root), str(root), "--chdir", str(root)]
    args += project_git(root, scratch / "build-git")
    for source, target in [(home / ".cargo/bin", "/sandbox/.cargo/bin"),
                           (home / ".cargo/registry", "/sandbox/.cargo/registry"),
                           (home / ".rustup", "/sandbox/.rustup")]:
        if source.exists():
            args += ["--ro-bind", str(source), target]
    node = node.resolve(strict=True)
    npm = node.parent.parent / "lib/node_modules/npm"
    if not npm.is_dir():
        raise RuntimeError("The configured Node installation has no trusted npm")
    args += ["--ro-bind", str(node), "/sandbox/bin/node", "--ro-bind", str(npm), "/sandbox/npm",
             "--setenv", "CARGO_HOME", "/sandbox/.cargo", "--setenv", "RUSTUP_HOME", "/sandbox/.rustup",
             "--setenv", "CARGO_TARGET_DIR", str(root / "target"), "--setenv", "CARGO_NET_OFFLINE", "true",
             "--setenv", "npm_config_offline", "true", "--setenv", "GIT_CONFIG_NOSYSTEM", "1",
             "/bin/bash", "-c", 'npm() { /sandbox/bin/node /sandbox/npm/bin/npm-cli.js "$@"; }; export -f npm; ' + command]
    return subprocess.call(args)


def preview(root, state):
    scratch = Path(f"/tmp/dispatch-{root.name}")
    scratch.mkdir(parents=True, exist_ok=True, mode=0o700)
    if state is None:
        state = Path(tempfile.mkdtemp(prefix="browser-state-", dir=scratch))
    else:
        state = state.resolve(strict=True)
        if not state.is_relative_to(scratch):
            raise ValueError("Preview state leaves the assigned scratch directory")
    bridge = Path(tempfile.mkdtemp(prefix="browser-bridge-", dir=scratch))
    resources = Path(__file__).resolve().parent
    child = None

    class Handler(socketserver.BaseRequestHandler):
        def handle(self):
            try:
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as upstream:
                    upstream.connect(str(bridge / "application.sock"))
                    relay(self.request, upstream)
            except (OSError, TimeoutError):
                pass

    server = TcpServer(("127.0.0.1", 0), Handler)
    origin = f"http://127.0.0.1:{server.server_address[1]}"
    args = runtime() + ["--ro-bind", str(root / ".build"), "/artifact", "--bind", str(state), "/state",
                        "--bind", str(bridge), "/bridge", "--ro-bind", str(resources), "/trusted",
                        "/usr/bin/python3", "-I", "/trusted/host-sandbox.py", "--inner-preview", json.dumps({"origin": origin})]
    try:
        child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        for _ in range(400):
            if child.poll() is not None:
                raise RuntimeError("Isolated preview exited before readiness")
            try:
                with urllib.request.urlopen(origin + "/api/health", timeout=0.5) as response:
                    if response.status == 200:
                        print(json.dumps({"origin": origin, "stateRoot": str(state), "bridgeRoot": str(bridge)}), flush=True)
                        break
            except OSError:
                time.sleep(0.1)
        else:
            raise RuntimeError("Isolated preview did not become ready")
        # The Rust process owns this pipe and cancels the complete process group on lease loss.
        sys.stdin.buffer.read()
        return 0
    finally:
        if child:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
        server.shutdown()
        server.server_close()
        shutil.rmtree(bridge, ignore_errors=True)
        # Native verification state lives until its outer ticket cleanup; fixture state is ephemeral.
        if state.name.startswith("browser-state-"):
            shutil.rmtree(state, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inner-preview")
    parser.add_argument("--mode", choices=["build", "preview"])
    parser.add_argument("--worktree", type=Path)
    parser.add_argument("--node", type=Path)
    parser.add_argument("--state-root", type=Path)
    parser.add_argument("--command")
    args = parser.parse_args()
    if args.inner_preview:
        return inner_preview(json.loads(args.inner_preview))
    root = args.worktree.resolve(strict=True)
    if root.parent.name != "worktrees" or not root.name.startswith("support-") or not (root / ".git").is_file():
        raise ValueError("Sandbox requires an assigned support worktree")
    if args.mode == "build":
        return build(root, args.node, args.command)
    return preview(root, args.state_root)


if __name__ == "__main__":
    sys.exit(main())
