"""The launcher's cached executable, held until the command and its children end."""
from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


@contextmanager
def locked_directory(path: Path, operation: int):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        fcntl.flock(fd, operation)
        yield fd
    finally:
        os.close(fd)


def prune(cache: Path):
    """Keep the three newest entries and any older entry a command still holds.

    The caller holds the cache directory, so opening an entry and holding it
    cannot race with its removal. No lock files survive an entry's removal.
    """
    entries = sorted(
        (entry for entry in cache.iterdir() if entry.is_dir() and not entry.is_symlink()),
        key=lambda entry: (entry.stat().st_mtime_ns, entry.name),
        reverse=True,
    )
    for entry in entries[3:]:
        try:
            with locked_directory(entry, fcntl.LOCK_EX | fcntl.LOCK_NB):
                shutil.rmtree(entry)
        except BlockingIOError:
            pass


def main():
    source, cache, version = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    cache.mkdir(parents=True, exist_ok=True)
    entry = cache / version
    binary = entry / "dispatchdev"
    # Hold the entry before looking for its executable or starting its build.
    # All launchers open it under the cache lock; eviction takes that lock too.
    with locked_directory(cache, fcntl.LOCK_EX):
        entry.mkdir(exist_ok=True)
        held = os.open(entry, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        fcntl.flock(held, fcntl.LOCK_SH)
    if not os.access(binary, os.X_OK):
        result = subprocess.run([
            "cargo", "build", "--quiet", "--locked", "-p", "dispatchdev-cli",
            "--bin", "dispatchdev", "--target-dir", str(source / "target"),
        ], cwd=source, stdout=sys.stderr)
        if result.returncode:
            sys.exit(result.returncode if result.returncode > 0 else 128 - result.returncode)
        with tempfile.NamedTemporaryFile(dir=entry, delete=False) as partial:
            partial_path = Path(partial.name)
        try:
            shutil.copyfile(source / "target/debug/dispatchdev", partial_path)
            partial_path.chmod(0o755)
            with locked_directory(cache, fcntl.LOCK_EX):
                # Another launcher of these same inputs may have finished first.
                # Replacing its executable would unlink a running process's image.
                if not os.access(binary, os.X_OK):
                    partial_path.replace(binary)
                prune(cache)
        finally:
            partial_path.unlink(missing_ok=True)
    # exec preserves the lock, without a supervisor or liveness marker. The
    # kernel lets it go even when the command is killed without cleaning up.
    os.set_inheritable(held, True)
    os.execv(binary, [str(binary), *sys.argv[4:]])


if __name__ == "__main__":
    main()
