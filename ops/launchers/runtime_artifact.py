"""The launchers' way to the Rust host manager, from a checkout or an installed management
directory. Artifact policy and the updaters live in ops/host-manager."""
import functools
import json
import os
from pathlib import Path
import subprocess
import sys

def require(value, message):
    if not value:
        raise RuntimeError(message)


def command(*args, cwd=None, timeout=120, binary=False):
    result = subprocess.run(args, cwd=cwd, text=not binary, capture_output=True, timeout=timeout)
    if result.returncode:
        problem = result.stderr if isinstance(result.stderr, str) else result.stderr.decode(errors="replace")
        raise RuntimeError(f"{' '.join(args[:3])} failed: {problem.strip()[-600:]}")
    return result.stdout if binary else result.stdout.strip()


def prebuilt_host(root):
    """The host the tools job of this ref or of main built from identical inputs, restored here.

    Only the workspace's own `.ci-tools` directory is trusted, and only on CI. This file is also
    installed alone under `management`, so the rule is kept here rather than imported.
    """
    tools = os.environ.get("DISPATCH_CI_TOOLS")
    if os.environ.get("CI") != "true" or not tools or Path(tools) != root / ".ci-tools":
        return None
    binary = root / ".ci-tools/tools/dispatch-host"
    if binary.is_symlink() or not binary.is_file() or not os.access(binary, os.X_OK):
        return None
    return binary


@functools.cache
def host_binary():
    here = Path(__file__).resolve().parent
    root = here.parent.parent
    # A checkout runs this from ops/launchers. An installed copy runs from its management
    # directory, which may sit inside a checkout (Dev's), so only this file's own place counts.
    if here.name == "launchers" and here.parent.name == "ops" and (root / "ops/host-manager/Cargo.toml").is_file():
        # Build from this checkout, never search the candidate being verified.
        restored = prebuilt_host(root)
        if restored is not None:
            return restored
        # Selecting the whole workspace resolves dependency features as the workspace build
        # does, so right after one this reuses its output instead of compiling them again.
        subprocess.check_call(["cargo", "build", "--locked", "--release", "--workspace", "--bin", "dispatch-host"],
                              cwd=root, stdout=sys.stderr)
        metadata = json.loads(command("cargo", "metadata", "--locked", "--no-deps", "--format-version=1", cwd=root))
        return Path(metadata["target_directory"]) / "release/dispatch-host"
    require(here.name == "management", "Run host tooling from a checkout or installed management directory")
    binary = here / "dispatch-host"
    require(binary.exists(), "Installed host management is missing; install it from a checkout with --install-management")
    info = binary.lstat()
    require(binary.is_file() and not binary.is_symlink() and info.st_nlink == 1
            and info.st_uid == os.getuid() and info.st_mode & 0o077 == 0,
            "Private installed host executable required")
    return binary


def host(*args, value=None):
    process = subprocess.Popen([str(host_binary()), "host", *map(str, args)],
                               stdin=subprocess.PIPE if value is not None else subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        stdout, stderr = process.communicate(None if value is None else json.dumps(value), timeout=600)
    except BaseException:
        process.kill()
        process.wait()
        raise
    require(process.returncode == 0, stderr.strip() or "Host management failed")
    # What the host reports alongside a success, such as an install, reaches the journal.
    sys.stderr.write(stderr)
    return json.loads(stdout)


def install_management(live, environment="dev"):
    """Compatibility adapter; Rust installs the binary and embedded launchers together."""
    host(environment, "--root", Path(live), "--install-management")


if __name__ == "__main__":
    try:
        print(json.dumps(host(*sys.argv[1:])))
    except (OSError, ValueError, RuntimeError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
