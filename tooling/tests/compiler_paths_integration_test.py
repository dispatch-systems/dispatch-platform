"""Real compiler checks, scheduled outside the source-only rules pass."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from compiler_paths_test import ROOT, config, gate


class CompilerPathIntegrationTests(unittest.TestCase):
    def test_real_cargo_remaps_dependencies_and_rebuilds_when_policy_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "checkout"
            dependency = Path(directory) / "private-cargo/registry/dependency"
            for path in [root / "src", root / ".cargo", root / "tooling/build", dependency / "src"]:
                path.mkdir(parents=True)
            wrapper = root / "tooling/build/rustc-remap.py"
            shutil.copy2(ROOT / "tooling/build/rustc-remap.py", wrapper)
            (dependency / "Cargo.toml").write_text('[package]\nname="path-probe"\nversion="0.0.0"\nedition="2024"\n')
            (dependency / "src/lib.rs").write_text('pub fn origin() -> &\'static str { file!() }\n')
            (root / "Cargo.toml").write_text('[package]\nname="compiler-path-probe"\nversion="0.0.0"\nedition="2024"\n'
                                              '[dependencies]\npath-probe={path="../private-cargo/registry/dependency"}\n')
            (root / "src/main.rs").write_text('fn main() { println!("{}", path_probe::origin()); }\n')
            env = {k: v for k, v in os.environ.items() if not k.startswith(("CARGO_", "RUSTFLAGS", "RUSTC"))}
            env["CARGO_HOME"] = str(Path(directory) / "private-cargo")
            binary = root / "target/debug/compiler-path-probe"
            for destination in ["cargo", "updated-cargo"]:
                if destination != "cargo":
                    wrapper.write_text(wrapper.read_text().replace('/dispatch-build/cargo', '/dispatch-build/updated-cargo'))
                flag = config(wrapper)["build"]["rustflags"][0]
                (root / ".cargo/config.toml").write_text('[build]\nrustc-wrapper="tooling/build/rustc-remap.py"\n'
                                                         f'rustflags=["{flag}"]\n')
                # Starting in a member source directory also checks Cargo's config path resolution.
                result = subprocess.run(["cargo", "build", "--offline"], cwd=root / "src", env=env,
                                        capture_output=True, text=True, timeout=60)
                self.assertEqual(result.returncode, 0, result.stderr)
                output = subprocess.check_output([str(binary)], text=True).strip()
                self.assertEqual(output, f"/dispatch-build/{destination}/registry/dependency/src/lib.rs")
                self.assertFalse(gate.has_build_paths(binary.read_bytes(), root, env))


if __name__ == "__main__":
    unittest.main()
