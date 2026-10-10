use crate::{Result, Runner};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};
pub type Environment = BTreeMap<String, String>;
// The workspace, the toolchain and the compiler wrapper, and the launchers the host manager
// embeds for fresh management installs.
const INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rust-toolchain",
    "tooling/build/rustc-remap.py",
    "ops/launchers/runtime_artifact.py",
    "ops/launchers/update-dev.py",
    "ops/launchers/update-production.py",
];
/// Every directory the Rust workspace compiles from: crates, their modules, the files they
/// embed, migrations and tests.
pub const SOURCE_ROOTS: &[&str] = &[
    "app",
    "core",
    "collectors",
    "features",
    "mcp",
    "ops/host-manager",
    "tooling/cli",
    "tooling/shared",
];
/// Whether a file in the checkout can change what the backend's builds compile. `cargo build`
/// builds every member but compiles no test target, and every test module is mounted for tests
/// only, so neither test code nor the development CLI's own files reach the backend. A
/// manifest does, the CLI's too: any member's features reach the whole build.
fn reaches_backend(file: &Path) -> bool {
    !file.components().any(|part| part.as_os_str() == "tests")
        && (!file.starts_with("tooling/cli") || file == Path::new("tooling/cli/Cargo.toml"))
}
/// Frontend code shares the owners' directories with Rust but never compiles into it: a
/// `frontend/` folder, or a TypeScript or CSS file. The structure rules forbid Rust from
/// embedding either, so neither belongs in a Rust input.
pub fn frontend(path: &Path) -> bool {
    path.components()
        .any(|part| ["frontend", "node_modules"].contains(&part.as_os_str().to_str().unwrap_or("")))
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| ["ts", "tsx", "mts", "cts", "css"].contains(&extension))
}
/// Every file under `dir` but frontend code, judged by its path inside the checkout at `root`:
/// the checkout's own path may name any folder.
fn walk(root: &Path, dir: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if frontend(path.strip_prefix(root).unwrap_or(&path)) && !path.is_symlink() {
            continue;
        }
        if path.is_symlink() || !path.is_dir() {
            files.insert(path);
        } else {
            walk(root, &path, files)?;
        }
    }
    Ok(())
}
fn configs(root: &Path, env: &Environment) -> Result<Vec<PathBuf>> {
    let home = match env.get("CARGO_HOME") {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(env.get("HOME").ok_or("HOME or CARGO_HOME required")?).join(".cargo"),
    };
    Ok(root
        .ancestors()
        .map(|p| p.join(".cargo"))
        .chain([home])
        .flat_map(|p| [p.join("config"), p.join("config.toml")])
        .collect())
}
fn flags(env: &Environment) -> Environment {
    env.iter()
        .filter(|(key, _)| {
            ["CARGO_", "RUST", "CC_", "CXX_", "PKG_CONFIG"]
                .iter()
                .any(|prefix| key.starts_with(prefix))
                || [
                    "CC",
                    "CXX",
                    "CFLAGS",
                    "CPPFLAGS",
                    "CXXFLAGS",
                    "LDFLAGS",
                    "AR",
                    "RANLIB",
                    // The distribution, not the weekly image build: the compiler, C compiler and
                    // linker versions that image updates can change are fingerprinted directly.
                    "ImageOS",
                    "RUNNER_OS",
                    "RUNNER_ARCH",
                ]
                .contains(&key.as_str())
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
pub fn fingerprint(
    root: &Path,
    profile: &str,
    compiler: &str,
    env: &Environment,
) -> Result<String> {
    // The assessment fixture is a test target, so only its key reads test code.
    let backend = matches!(profile, "release" | "debug");
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&(
        6,
        profile,
        compiler,
        std::env::consts::OS,
        std::env::consts::ARCH,
        flags(env),
    ))?);
    let mut files: BTreeSet<_> = INPUTS.iter().map(|name| root.join(name)).collect();
    for source in SOURCE_ROOTS {
        walk(root, &root.join(source), &mut files)?;
    }
    walk(root, &root.join(".cargo"), &mut files)?;
    files.extend(configs(root, env)?);
    for file in files {
        let inside = file.strip_prefix(root).ok();
        if backend && inside.is_some_and(|inside| !reaches_backend(inside)) {
            continue;
        }
        if file.is_file() {
            let name = inside
                .unwrap_or(&file)
                .to_str()
                .ok_or("Non-UTF8 Rust input path")?;
            digest.update(name.as_bytes());
            digest.update([0]);
            digest.update(fs::read(&file)?);
            digest.update([0]);
        }
    }
    Ok(crate::to_hex(&digest.finalize()))
}
fn normalized(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}
fn external(value: &toml::Value, manifest: &Path, sources: &[PathBuf]) -> bool {
    match value {
        toml::Value::Table(values) => values.iter().any(|(key, value)| {
            (key == "path"
                && value.as_str().is_some_and(|path| {
                    let path = manifest.parent().unwrap().join(path);
                    let path = path.canonicalize().unwrap_or_else(|_| normalized(&path));
                    !sources.iter().any(|source| path.starts_with(source))
                }))
                || external(value, manifest, sources)
        }),
        toml::Value::Array(values) => values
            .iter()
            .any(|value| external(value, manifest, sources)),
        _ => false,
    }
}
/// Only the repository's exact path-remapping config may use the shared binary cache.
fn remap_config(root: &Path, file: &Path) -> bool {
    let wrapper = root.join("tooling/build/rustc-remap.py");
    // The file itself or any directory between it and the checkout's root.
    let linked = |path: &Path| {
        path.ancestors()
            .take_while(|ancestor| *ancestor != root)
            .any(Path::is_symlink)
    };
    if file != root.join(".cargo/config.toml") || linked(file) || linked(&wrapper) {
        return false;
    }
    let (Ok(bytes), Ok(text)) = (fs::read(wrapper), fs::read_to_string(file)) else {
        return false;
    };
    let hash = crate::to_hex(&Sha256::digest(bytes));
    let expected: toml::Value = toml::from_str(&format!(
        "[build]\nrustc-wrapper = 'tooling/build/rustc-remap.py'\nrustflags = ['--cfg=dispatch_path_policy_{hash}']\n"
    ))
    .unwrap();
    toml::from_str::<toml::Value>(&text).is_ok_and(|config| config == expected)
}
pub fn eligible(root: &Path, env: &Environment, allow_ci: bool) -> Result<bool> {
    let nonempty = |key: &str| env.get(key).is_some_and(|value| !value.is_empty());
    if (nonempty("CI") && !allow_ci)
        || nonempty("DISPATCH_DISABLE_RUST_CACHE")
        || nonempty("CARGO_TARGET_DIR")
        // Even empty overrides replace the policy fingerprint in build.rustflags.
        || env.contains_key("RUSTFLAGS")
        || env.contains_key("CARGO_ENCODED_RUSTFLAGS")
        || env.keys().any(|key| {
            key.starts_with("CARGO_SOURCE_")
                || key.starts_with("CARGO_BUILD_")
                || (key.starts_with("CARGO_TARGET_") && key.ends_with("_RUSTFLAGS"))
                || [
                    "RUSTC",
                    "RUSTC_WRAPPER",
                    "RUSTC_WORKSPACE_WRAPPER",
                    "CC",
                    "CXX",
                    "AR",
                    "RANLIB",
                ]
                .contains(&key.as_str())
        })
        || configs(root, env)?
            .iter()
            .any(|p| (p.exists() || p.is_symlink()) && !remap_config(root, p))
    {
        return Ok(false);
    }
    let workspace: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    let members: Vec<_> = workspace
        .get("workspace")
        .and_then(|v| v.get("members"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .collect();
    // Every member, globs included, must sit inside the fingerprinted source roots.
    let within = |member: &str| {
        let literal = member
            .split(['*', '?', '['])
            .next()
            .unwrap_or_default()
            .trim_end_matches('/');
        !literal.is_empty()
            && !literal
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            && SOURCE_ROOTS
                .iter()
                .any(|source| literal == *source || literal.starts_with(&format!("{source}/")))
    };
    if members.is_empty() || !members.iter().all(|member| within(member)) {
        return Ok(false);
    }
    let mut files = BTreeSet::new();
    for source in SOURCE_ROOTS {
        walk(root, &root.join(source), &mut files)?;
    }
    if files
        .iter()
        .any(|p| p.is_symlink() || p.file_name().is_some_and(|n| n == "build.rs"))
    {
        return Ok(false);
    }
    let sources: Vec<_> = SOURCE_ROOTS
        .iter()
        .map(|source| root.join(source))
        .collect();
    for manifest in [root.join("Cargo.toml")].into_iter().chain(
        files
            .into_iter()
            .filter(|p| p.file_name().is_some_and(|n| n == "Cargo.toml")),
    ) {
        let value: toml::Value = toml::from_str(&fs::read_to_string(&manifest)?)?;
        let build = value.get("package").and_then(|v| v.get("build"));
        if build.is_some_and(|v| v.as_bool() != Some(false))
            || external(&value, &manifest, &sources)
        {
            return Ok(false);
        }
    }
    Ok(true)
}
/// The toolchain that shapes the binary: Rust, the C compiler and the linker, by version.
pub(super) fn compiler(root: &Path, runner: &dyn Runner) -> Result<String> {
    Ok(format!(
        "{}\n{}\n{}",
        String::from_utf8(runner.command(&["rustc", "-vV"], Some(root), 30)?)?.trim(),
        String::from_utf8(runner.command(&["cc", "--version"], Some(root), 30)?)?.trim(),
        String::from_utf8(runner.command(&["ld", "--version"], Some(root), 30)?)?.trim()
    ))
}
pub fn key(root: &Path, profile: &str, env: &Environment, runner: &dyn Runner) -> Result<String> {
    fingerprint(root, profile, &compiler(root, runner)?, env)
}
