//! Host-validated tools projected read-only into the dedicated collection verifier.
//! Normal browser construction retains its root ownership checks. Only the verifier CLI
//! accepts this host-written receipt after checking its namespace and mount boundary.
use super::*;
use sha2::{Digest, Sha256};
use std::{io::Read, os::fd::AsRawFd};

const RECEIPT: &str = "/run/support-provider/browser-authority.json";
const DISPLAY: &str = "/usr/bin/Xvfb";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Executable {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Executables {
    browser: Executable,
    sandbox: Executable,
    display: Executable,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Namespaces {
    pid: String,
    net: String,
    mnt: String,
    ipc: String,
    uts: String,
    user: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    source_sha: String,
    backend_sha256: String,
    host_namespaces: Namespaces,
    executables: Executables,
}

/// Constructible only from the fixed, host-written, read-only receipt. It cannot be
/// deserialized from a platform request or selected by an environment variable.
pub struct VerificationAuthority {
    tools: Executables,
    executable: PathBuf,
}
impl VerificationAuthority {
    pub fn load(config: &crate::foundation::config::Config) -> Result<Self> {
        ensure(
            config.development && config.environment == "preview" && !config.fixture,
            "support_verification_dev_only",
            403,
        )?;
        let path = Path::new(RECEIPT);
        let stat = fs::symlink_metadata(path)?;
        projection_check(
            stat.is_file()
                && stat.nlink() == 1
                && stat.uid() == unsafe { libc::geteuid() }
                && stat.mode() & 0o777 == 0o400
                && path.canonicalize()? == path,
        )?;
        read_only(path)?;
        let mut bytes = Vec::new();
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?
            .take(16385)
            .read_to_end(&mut bytes)?;
        projection_check(bytes.len() <= 16384)?;
        let receipt: Receipt = serde_json::from_slice(&bytes)?;
        projection_check(
            receipt.schema_version == 1
                && hex_digest(&receipt.source_sha, 40)
                && config.source.commit.as_deref() == Some(receipt.source_sha.as_str()),
        )?;
        confined(&receipt.host_namespaces)?;
        let executable = std::env::current_exe()?;
        checked_file(&executable, &receipt.backend_sha256)?;
        projection_check(
            receipt.executables.browser.path == config.browseros
                && receipt
                    .executables
                    .browser
                    .path
                    .file_name()
                    .is_some_and(|name| name == "browseros")
                && receipt
                    .executables
                    .browser
                    .path
                    .parent()
                    .and_then(Path::parent)
                    == Some(Path::new("/opt/dispatch-browseros"))
                && receipt.executables.sandbox.path == config.sandbox
                && matches!(
                    receipt.executables.sandbox.path.to_str(),
                    Some("/usr/bin/bwrap" | "/usr/local/libexec/dispatch-dev/bwrap")
                )
                && receipt.executables.display.path == Path::new(DISPLAY),
        )?;
        for tool in [
            &receipt.executables.browser,
            &receipt.executables.sandbox,
            &receipt.executables.display,
        ] {
            checked_file(&tool.path, &tool.sha256)?;
        }
        Ok(Self {
            tools: receipt.executables,
            executable,
        })
    }
    pub(crate) fn runtime(self, runs: &Path, capacity: usize) -> Result<Runtime> {
        ensure(
            (1..=16).contains(&capacity),
            "invalid_browser_capacity",
            400,
        )?;
        Ok(Runtime {
            browser: self.tools.browser.path.clone(),
            sandbox: self.tools.sandbox.path.clone(),
            executable: self.executable.clone(),
            runs: db::private_dir(runs)?,
            slots: Arc::new(Semaphore::new(capacity)),
            verification: Some(self),
        })
    }
    pub(super) fn display(&self) -> Result<()> {
        checked_file(&self.tools.display.path, &self.tools.display.sha256)
    }
}

fn projection_check(valid: bool) -> Result<()> {
    ensure(valid, "support_browser_projection_invalid", 403)
}
fn hex_digest(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn confined(host: &Namespaces) -> Result<()> {
    for (name, expected) in [
        ("pid", &host.pid),
        ("net", &host.net),
        ("mnt", &host.mnt),
        ("ipc", &host.ipc),
        ("uts", &host.uts),
        ("user", &host.user),
    ] {
        let prefix = format!("{name}:[");
        projection_check(
            expected
                .strip_prefix(&prefix)
                .and_then(|value| value.strip_suffix(']'))
                .is_some_and(|value| {
                    !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit())
                })
                && fs::read_link(format!("/proc/self/ns/{name}"))? != Path::new(expected),
        )?;
    }
    let status = fs::read_to_string("/proc/self/status")?;
    for name in ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"] {
        projection_check(status.lines().any(|line| {
            line.strip_prefix(&format!("{name}:")).is_some_and(|value| {
                !value.trim().is_empty() && value.trim().bytes().all(|c| c == b'0')
            })
        }))?;
    }
    projection_check(status.lines().any(|line| {
        line.strip_prefix("NoNewPrivs:")
            .is_some_and(|value| value.trim() == "1")
    }))
}
fn checked_file(path: &Path, digest: &str) -> Result<()> {
    let stat = fs::symlink_metadata(path)?;
    projection_check(
        path.is_absolute()
            && path.canonicalize()? == path
            && stat.is_file()
            && stat.mode() & 0o111 != 0
            && stat.mode() & 0o022 == 0
            && hex_digest(digest, 64),
    )?;
    read_only(path)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut sha = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let length = file.read(&mut bytes)?;
        if length == 0 {
            break;
        }
        sha.update(&bytes[..length]);
    }
    projection_check(crypto::hex(&sha.finalize()) == digest)
}
fn read_only(path: &Path) -> Result<()> {
    // Query the opened file's actual mount, including stacked bind mounts. A matching
    // read-only parent in mountinfo must never hide a writable mount at the file itself.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut flags: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatvfs(file.as_raw_fd(), &mut flags) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    projection_check(flags.f_flag & libc::ST_RDONLY != 0)
}

#[cfg(test)]
#[path = "../../../tests/backend/browser/browseros/projection.rs"]
mod tests;
