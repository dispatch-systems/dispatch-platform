#!/usr/bin/env python3
"""Give compiler-generated paths stable names without changing runtime file access, and compile
through sccache where it is installed, so checkouts and test runs share the crates they compile."""
import os
from pathlib import Path
import shutil
import sys

# Cargo's own settings, which rustc never reads but sccache keys a compile by: without them, a
# test run's target folder of its own and the checkout's shared one compile a crate alike.
CARGO_ONLY = ("CARGO_TARGET_DIR", "CARGO_INCREMENTAL")
SOURCE = "/dispatch-build/source"


def mappings(root, env):
    home = Path(env.get("HOME") or Path.home())
    locations = [
        (home, "/dispatch-build/home"),
        (Path(env.get("CARGO_HOME") or home / ".cargo"), "/dispatch-build/cargo"),
        (Path(env.get("RUSTUP_HOME") or home / ".rustup"), "/dispatch-build/rustup"),
        (Path(root), SOURCE),
    ]
    paths = {}
    for location, replacement in locations:
        # Cover both the spelling Cargo uses and a symlink's physical location.
        for source in [location.absolute(), location.resolve()]:
            if source == Path(source.anchor):
                raise ValueError("Refusing to remap a filesystem root")
            paths[str(source)] = replacement
    # rustc applies the last matching prefix: specific roots must beat the home directory.
    return [f"--remap-path-prefix={source}={paths[source]}"
            for source in sorted(paths, key=lambda value: (len(value), value))]


def names_checkout(root, env, cwd):
    """Whether a compile can write the checkout's paths into what it makes: one of the checkout's
    own crates, which Cargo compiles from inside it, or a crate whose build script writes there.
    A registry crate without one can't, and leaving the checkout out of its arguments lets every
    checkout share its compile."""
    inside = lambda path: Path(path).resolve().is_relative_to(Path(root).resolve())
    return inside(cwd) or bool(env.get("OUT_DIR")) and inside(env["OUT_DIR"])


def command(root, compiler, arguments, env, cwd, cache):
    """The compile, through `cache` when there is one, with the mappings it can need."""
    flags = mappings(root, env)
    if not names_checkout(root, env, cwd):
        flags = [flag for flag in flags if not flag.endswith(f"={SOURCE}")]
    return [*([cache] if cache else []), compiler, *arguments, *flags]


def environment(env):
    """The compile's environment: Cargo's own settings out, and sccache compiling here itself
    when its server can't be reached."""
    kept = {name: value for name, value in env.items() if name not in CARGO_ONLY}
    kept.setdefault("SCCACHE_IGNORE_SERVER_IO_ERROR", "1")
    return kept


def main():
    if len(sys.argv) < 2:
        raise SystemExit("Cargo must supply a compiler executable")
    root = Path(__file__).resolve().parents[2]
    compiler, *arguments = sys.argv[1:]
    run = command(root, compiler, arguments, os.environ, os.getcwd(), shutil.which("sccache"))
    os.execvpe(run[0], run, environment(os.environ))


if __name__ == "__main__":
    main()
