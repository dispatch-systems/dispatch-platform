use crate::{Result, require};
use serde::Deserialize;
use std::{env, fs, path::PathBuf};
use url::Url;

/// Private configuration, written by host management with mode 0600, never by an agent.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub origins: Vec<Origin>,
    pub workspace_root: PathBuf,
    pub runtime_root: PathBuf,
    pub hermes_source: PathBuf,
    pub hermes_python: PathBuf,
    pub repository: String,
    pub collection_bridge: PathBuf,
    pub node: PathBuf,
    /// Production's server, read over SSH by its `dispatch-reader` account. Absent, Production
    /// tickets are investigated without it.
    #[serde(default)]
    pub production_reader: Option<ProductionReader>,
}

/// The reader's own key can only run Production's fixed reads (`ops/scripts/production-reader.py`),
/// only from this host's Tailscale address; the host key is pinned.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductionReader {
    pub host: String,
    pub user: String,
    pub key_file: PathBuf,
    pub known_hosts: PathBuf,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Origin {
    pub environment: String,
    pub url: String,
    pub token_file: PathBuf,
}

impl Config {
    pub fn load() -> Result<Self> {
        let file = env::var_os("SUPPORT_WORKER_CONFIG").ok_or("SUPPORT_WORKER_CONFIG missing")?;
        let value: Self = serde_json::from_slice(&read_private(&PathBuf::from(file))?)?;
        value.validate()?;
        fs::create_dir_all(&value.runtime_root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&value.runtime_root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        require(!self.origins.is_empty(), "No support origins configured")?;
        for origin in &self.origins {
            require(
                matches!(origin.environment.as_str(), "dev" | "production"),
                "Invalid support origin environment",
            )?;
            let parsed = Url::parse(&origin.url)?;
            require(
                parsed.scheme() == "https" || parsed.host_str() == Some("127.0.0.1"),
                "Support origins must use HTTPS",
            )?;
            require(
                parsed.username().is_empty()
                    && parsed.password().is_none()
                    && parsed.query().is_none()
                    && parsed.fragment().is_none()
                    && parsed.path() == "/",
                "Support origin must be an origin without credentials or a path",
            )?;
            require(
                origin.token_file.is_absolute(),
                "Support token paths must be absolute",
            )?;
        }
        require(
            self.origins
                .iter()
                .map(|origin| &origin.environment)
                .collect::<std::collections::HashSet<_>>()
                .len()
                == self.origins.len(),
            "Each support environment needs a single configured origin",
        )?;
        require(
            self.workspace_root.is_absolute()
                && self.runtime_root.is_absolute()
                && self.hermes_source.is_absolute()
                && self.hermes_python.is_absolute()
                && self.collection_bridge.is_absolute()
                && self.node.is_absolute(),
            "Support paths must be absolute",
        )?;
        require(
            self.repository.split('/').count() == 2
                && self
                    .repository
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_/".contains(c)),
            "Invalid support repository",
        )?;
        if let Some(reader) = &self.production_reader {
            require(
                !reader.host.is_empty()
                    && reader
                        .host
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c))
                    && !reader.user.is_empty()
                    && reader
                        .user
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || "-_".contains(c))
                    && reader.key_file.is_absolute()
                    && reader.known_hosts.is_absolute(),
                "Invalid Production reader",
            )?;
        }
        Ok(())
    }
}

/// Service authority must never be loaded from a shared file or a symlink.
pub fn read_private(path: &std::path::Path) -> Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    require(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.permissions().mode() & 0o077 == 0,
        "Support configuration and tokens must be private regular files owned by the service user",
    )?;
    let mut data = vec![];
    file.read_to_end(&mut data)?;
    Ok(data)
}
