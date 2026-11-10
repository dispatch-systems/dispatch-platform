//! Read the task's fixture receipt without starting it or creating runtime state.
use crate::{Result, Runner, api, build, workspace};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub commit: String,
    pub tree: String,
    pub dirty: bool,
    pub fingerprint: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIdentity {
    pub pid: i32,
    pub started: String,
    #[serde(skip_serializing)]
    pub command: String,
}
#[derive(Deserialize)]
struct Record {
    version: Option<u32>,
    root: Option<String>,
    process: Option<ProcessIdentity>,
    source: Option<Source>,
    origin: String,
    link: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub origin: Option<String>,
    #[serde(skip)]
    pub link: Option<String>,
    pub process: Option<ProcessIdentity>,
    pub source: Option<Source>,
    pub checkout: Source,
    pub source_matches: Option<bool>,
    pub backend_matches: Option<bool>,
    pub reason: Option<String>,
}
impl Status {
    pub fn print(&self) {
        println!(
            "Fixture process: {}",
            if self.running {
                "running"
            } else {
                "not verified running"
            }
        );
        if let Some(origin) = &self.origin {
            println!("Origin: {origin}");
        }
        if let Some(process) = &self.process {
            println!("Process: {} (started {})", process.pid, process.started);
        }
        if let Some(source) = &self.source {
            println!(
                "Served commit: {}{}",
                source.commit,
                if source.dirty {
                    " (uncommitted source)"
                } else {
                    ""
                }
            );
        }
        println!(
            "Source matches checkout: {}",
            match self.source_matches {
                Some(true) => "yes",
                Some(false) => "no",
                None => "unknown",
            }
        );
        if let Some(reason) = &self.reason {
            println!("{reason}");
        }
    }
}

/// Whether the dev server serves `file` as it is on disk: a page's code, in an owner's `frontend`
/// folder, which Vite reads as it is saved. A change to it needs no new server.
fn live(file: &str) -> bool {
    file.split('/').any(|part| part == "frontend")
}
/// The checkout's commit, and the fingerprint of every file a server holds from its start: all
/// of them but the pages' code.
fn source(root: &Path, runner: &dyn Runner) -> Result<Source> {
    let git = |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(root));
    let commit = git(&["rev-parse", "HEAD"])?;
    let tree = git(&["rev-parse", "HEAD^{tree}"])?;
    let dirty = !git(&["--no-optional-locks", "status", "--porcelain", "-z"])?.is_empty();
    let listed = git(&[
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
    ])?;
    let mut files: Vec<_> = listed
        .split('\0')
        .filter(|file| !file.is_empty() && !live(file))
        .collect();
    files.sort_unstable();
    files.dedup();
    let mut hash = Sha256::new();
    for file in files {
        hash.update(file.as_bytes());
        hash.update([0]);
        let path = root.join(file);
        if path.is_symlink() {
            hash.update(b"link\0");
            hash.update(fs::read_link(path)?.as_os_str().as_encoded_bytes());
        } else if path.is_file() {
            hash.update(b"file\0");
            hash.update(digest(&path)?);
        } else {
            hash.update(b"missing\0");
        }
        hash.update([0]);
    }
    Ok(Source {
        commit,
        tree,
        dirty,
        fingerprint: crate::to_hex(&hash.finalize()),
    })
}
fn digest(path: &Path) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().to_vec())
}
/// Reuse the existing exact-input cache receipt; a stale or overridden backend has no proof.
fn backend_matches(root: &Path, runner: &dyn Runner) -> Option<bool> {
    if std::env::var_os("DISPATCH_TEST_BINARY").is_some() {
        return None;
    }
    let binary = root.join("target/debug/dispatch-backend");
    if !binary.is_file() {
        return None;
    }
    let env = std::env::vars().collect();
    if !build::eligible(root, &env, false).ok()? {
        return None;
    }
    let key = build::key(root, "debug", &env, runner).ok()?;
    let common = workspace::text(
        runner,
        &[
            "git",
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ],
        Some(root),
    )
    .ok()?;
    let receipt = fs::read_to_string(
        Path::new(&common)
            .join("dispatch-rust-builds")
            .join(key)
            .join("sha256"),
    )
    .ok()?;
    let expected = receipt
        .lines()
        .find_map(|line| line.strip_suffix("  dispatch-backend"))?;
    Some(crate::hex(expected, 64) && crate::to_hex(&digest(&binary).ok()?) == expected)
}

pub fn status(root: &Path, runner: &dyn Runner) -> Result<Status> {
    let root = fs::canonicalize(root)?;
    let mut status = Status {
        running: false,
        origin: None,
        link: None,
        process: None,
        source: None,
        checkout: source(&root, runner)?,
        source_matches: None,
        backend_matches: backend_matches(&root, runner),
        reason: None,
    };
    let path = root.join(api::RECORD);
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && (!metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
            || fs::canonicalize(path.parent().expect("receipt parent"))? != path.parent().unwrap())
    {
        status.reason = Some(
            "Fixture receipt must be this user's private regular file inside the checkout.".into(),
        );
        return Ok(status);
    }
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            status.reason = Some("No fixture receipt. Start npm run dev in this checkout.".into());
            return Ok(status);
        }
        Err(error) => return Err(error.into()),
    };
    let record: Record = match serde_json::from_str(&text) {
        Ok(record) => record,
        Err(_) => {
            status.reason = Some("Invalid fixture receipt; inspect .runtime/dev-server.json without printing its credentials.".into());
            return Ok(status);
        }
    };
    if record.version != Some(1) || record.root.as_deref() != root.to_str() {
        status.reason = Some("Fixture ownership/provenance is unknown: inspect the legacy or foreign receipt before restarting it.".into());
        return Ok(status);
    }
    let Some(process) = record.process.filter(|process| process.pid > 0) else {
        status.reason = Some("Fixture receipt has no valid process identity.".into());
        return Ok(status);
    };
    let pid = process.pid.to_string();
    let live = workspace::text(
        runner,
        &[
            "env", "LC_ALL=C", "ps", "-ww", "-p", &pid, "-o", "lstart=", "-o", "args=",
        ],
        Some(&root),
    );
    let expected = format!("{} {}", process.started, process.command);
    if live.is_err()
        || live.is_ok_and(|line| line.split_whitespace().collect::<Vec<_>>().join(" ") != expected)
    {
        status.reason = Some(
            "Fixture process ended or its identity changed; no sign-in link is exposed.".into(),
        );
        return Ok(status);
    }
    let valid_origin = record.origin.strip_prefix("http://").is_some_and(|host| {
        !host.contains(['/', '@', '?', '#'])
            && host
                .rsplit_once(':')
                .is_some_and(|(_, port)| port.parse::<u16>().is_ok_and(|port| port > 0))
    });
    if !valid_origin
        || !record
            .link
            .starts_with(&format!("{}/__preview/", record.origin))
    {
        status.reason = Some("Fixture receipt has an invalid origin or sign-in link.".into());
        return Ok(status);
    }
    status.running = true;
    status.origin = Some(record.origin);
    status.link = Some(record.link);
    status.process = Some(process);
    // The same files serve the same, whichever commit holds them: committing what a server
    // was started from changes nothing it serves.
    status.source_matches = record
        .source
        .as_ref()
        .map(|source| source.fingerprint == status.checkout.fingerprint);
    status.source = record.source;
    if status.source.is_none() {
        status.reason = Some("The running fixture has no verified served-source identity.".into());
    }
    Ok(status)
}

/// Stops the checkout's dev server, once `status` has checked it's the server its record names,
/// and waits for it to end: the process it stopped, if one ran.
pub fn stop(root: &Path, runner: &dyn Runner) -> Result<Option<i32>> {
    let status = status(root, runner)?;
    let Some(process) = status.process.filter(|_| status.running) else {
        return Ok(None);
    };
    unsafe { libc::kill(process.pid, libc::SIGTERM) };
    let stopping = std::time::Instant::now();
    while self::status(root, runner)?.running {
        crate::require(
            stopping.elapsed() < std::time::Duration::from_secs(30),
            &format!(
                "The dev server {} didn't stop: stop it, then run this again.",
                process.pid
            ),
        )?;
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(Some(process.pid))
}

#[cfg(test)]
mod tests;
