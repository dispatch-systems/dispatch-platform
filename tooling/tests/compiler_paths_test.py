import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


remap = load("remap", "tooling/build/rustc-remap.py")
gate = load("build_paths", "tooling/security/check-build-paths.py")


def config(wrapper):
    digest = hashlib.sha256(wrapper.read_bytes()).hexdigest()
    return {"build": {"rustc-wrapper": "tooling/build/rustc-remap.py",
                      "rustflags": ["--cfg=dispatch_path_policy_" + digest]}}


class CompilerPathTests(unittest.TestCase):
    def test_cargo_fingerprint_tracks_the_current_policy(self):
        self.assertEqual(tomllib.loads((ROOT / ".cargo/config.toml").read_text()),
                         config(ROOT / "tooling/build/rustc-remap.py"))

    def test_specific_paths_override_home_and_cover_symlink_locations(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / "operator"
            cargo = home / "cache with spaces"
            cargo.mkdir(parents=True)
            link = Path(directory) / "linked-cargo"
            link.symlink_to(cargo, target_is_directory=True)
            root = home / "project"
            flags = remap.mappings(root, {"HOME": str(home), "CARGO_HOME": str(link)})
            def mapped(path):
                for flag in reversed(flags):
                    before, after = flag.removeprefix("--remap-path-prefix=").rsplit("=", 1)
                    if str(path).startswith(before):
                        return after + str(path)[len(before):]
                return str(path)
            self.assertEqual(mapped(root / "src/main.rs"), "/dispatch-build/source/src/main.rs")
            self.assertEqual(mapped(cargo / "dependency.rs"), "/dispatch-build/cargo/dependency.rs")
            self.assertEqual(mapped(link / "dependency.rs"), "/dispatch-build/cargo/dependency.rs")
            self.assertEqual(mapped(home / ".rustup/library.rs"), "/dispatch-build/rustup/library.rs")
            self.assertEqual(mapped(home / "other.rs"), "/dispatch-build/home/other.rs")
        with self.assertRaises(ValueError):
            remap.mappings(ROOT, {"HOME": "/"})

    def test_only_a_compile_that_can_name_the_checkout_maps_it_and_sccache_runs_it_if_there(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / "operator"
            root = home / "checkout"
            registry = home / ".cargo/registry/src/serde"
            for path in [root / "target/debug/build/ring/out", registry]:
                path.mkdir(parents=True)
            env = {"HOME": str(home)}
            source = f"--remap-path-prefix={root}=/dispatch-build/source"
            def run(cwd, cache=None, **extra):
                return remap.command(root, "rustc", ["src/lib.rs"], {**env, **extra}, cwd, cache)
            # The checkout's own crates, and a crate whose build script writes into it, map it.
            self.assertIn(source, run(root))
            self.assertIn(source, run(registry, OUT_DIR=str(root / "target/debug/build/ring/out")))
            # A registry crate names nothing in it, so every checkout compiles it alike.
            plain = run(registry)
            self.assertNotIn(source, plain)
            self.assertIn(f"--remap-path-prefix={home / '.cargo'}=/dispatch-build/cargo", plain)
            self.assertEqual(plain[:2], ["rustc", "src/lib.rs"])
            self.assertEqual(run(registry, cache="/bin/sccache")[:3], ["/bin/sccache", "rustc", "src/lib.rs"])

    def test_cargo_settings_rustc_never_reads_leave_the_compile_alone(self):
        kept = remap.environment({"CARGO_TARGET_DIR": "/run/1", "CARGO_INCREMENTAL": "0",
                                  "CARGO_PKG_NAME": "serde", "PATH": "/bin"})
        self.assertEqual(kept, {"CARGO_PKG_NAME": "serde", "PATH": "/bin",
                                "SCCACHE_IGNORE_SERVER_IO_ERROR": "1"})

    def test_artifact_gate_rejects_home_and_custom_build_paths_without_printing_them(self):
        env = {"HOME": "/private/operator", "CARGO_HOME": "/cache/private-registry"}
        for mapped in [b"/dispatch-build/cargo/registry/a.rs", b"/dispatch-build/home/folder/a.rs"]:
            self.assertFalse(gate.has_build_paths(mapped, "/checkout", env))
        for private in ["/" + "home/operator/src/a.rs", "/" + "home/" + "a" * 32 + "/a.rs",
                        "/" + "Users/operator/a.rs", "/" + "Users/Example Person/a.rs",
                        "C:" + chr(92) + "Users" + chr(92) + "operator", "/cache/private-registry/a.rs",
                        "/checkout/src/main.rs", "/private/operator/a.rs"]:
            self.assertTrue(gate.has_build_paths(private.encode(), "/checkout", env))
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "bad-binary"
            private = "/" + "home/operator/private.rs"
            binary.write_bytes(b"\0" + private.encode())
            result = subprocess.run(["python3", str(ROOT / "tooling/security/check-build-paths.py"), str(binary)],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn(private, result.stdout + result.stderr)

    def test_adjacent_runtime_literals_are_not_one_compiler_path(self):
        # The backend checks these two directories while verifying browser isolation.
        literals = (b"/home" + b"/root" + b"DISPATCH_STATE_ROOTbrowser_isolation_required"
                    + b"pidnetmntipcutsBrowserOSServerBrowserClawServer" + b"/usr/bin/Xvfb")
        env = {"HOME": "/private/operator"}
        self.assertFalse(gate.has_build_paths(literals, "/checkout", env))
        long_home = "/" + "home/" + "a" * 40
        self.assertTrue(gate.has_build_paths((long_home + "/src/a.rs").encode(),
                                            "/checkout", {"HOME": long_home}))



if __name__ == "__main__":
    unittest.main()
