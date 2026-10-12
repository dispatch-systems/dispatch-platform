//! Host-authorized observation access. Credentials travel through private files and captured
//! subprocess output; the command's only public output is its origin, expiry and access mode.
use crate::{Result, Runner, require, workspace::Workspace};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Credential {
    origin: String,
    token: String,
    expires_at: u64,
    read_only: bool,
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Refuse ambiguous paths and existing files before minting any access. A private parent
/// makes the credential's name private too, and prevents another user replacing it.
fn output_parent(output: &Path) -> Result<&Path> {
    require(
        output.is_absolute()
            && output
                .components()
                .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
            && output.file_name().is_some(),
        "--out must name an absolute file without . or .. components.",
    )?;
    let parent = output.parent().ok_or("--out needs a parent directory.")?;
    let mut prefix = std::path::PathBuf::new();
    for component in parent.components() {
        prefix.push(component);
        let metadata = fs::symlink_metadata(&prefix)
            .map_err(|_| "--out's parent directory must already exist.")?;
        require(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "--out must not pass through a symbolic link.",
        )?;
    }
    let metadata = fs::metadata(parent)?;
    require(
        metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o077 == 0,
        "--out's parent directory must belong to this user and have mode 0700.",
    )?;
    require(
        matches!(fs::symlink_metadata(output), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "--out must name a new file; existing files and symbolic links are refused.",
    )?;
    Ok(parent)
}

/// Source only the installed host's config. Tracing, config output and backend diagnostics
/// are suppressed; neither an inherited shell setting nor a failure can print a secret.
fn mint_script(active: &str, config: &str, environment: &str, origin: &str) -> String {
    format!(
        "set +x\nset -eu\nexec 2>/dev/null\n\
         set -a\n. {config} >/dev/null 2>&1\nset +a\n\
         [ \"${{DISPATCH_ENVIRONMENT:-}}\" = {environment} ]\n\
         [ \"${{DISPATCH_ORIGIN:-}}\" = {origin} ]\n\
         observation_active={active}\ncd \"$observation_active\"\n\
         observation_private=$(mktemp -d /tmp/dispatch-observation.XXXXXXXX)\n\
         observation_file=\"$observation_private/session.json\"\n\
         trap 'rm -f -- \"$observation_file\"; rmdir -- \"$observation_private\"' EXIT HUP INT TERM\n\
         DISPATCH_ARTIFACT_ROOT=\"$observation_active\" \
         \"$observation_active/services/rust/dispatch-backend\" \
         observation-session \"$observation_file\" >/dev/null\n\
         cat -- \"$observation_file\"\n",
        environment = quote(environment),
        origin = quote(origin),
    )
}

/// Mint access for the installed Dev or Production instance, then save it without replacing
/// anything. The returned summary deliberately cannot contain the exchange credential.
pub fn run(ws: &Workspace, target: &str, output: &Path, runner: &dyn Runner) -> Result<Value> {
    let (environment, origin) = match target {
        "dev" => ("preview", "https://dispatchdev.dillonlille.com"),
        "production" => ("production", "https://admin.dspdispatch.com"),
        _ => return Err("The browser target is dev or production.".into()),
    };
    let parent = output_parent(output)?;
    let bytes = if target == "dev" {
        let dev = ws.dev();
        let active = dev.join(".build");
        let config = dev.join("config/platform.env");
        let script = mint_script(
            &quote(active.to_str().ok_or("Non-UTF8 workspace path")?),
            &quote(config.to_str().ok_or("Non-UTF8 workspace path")?),
            environment,
            origin,
        );
        runner.command(&["bash", "--noprofile", "--norc", "-c", &script], None, 60)
    } else {
        let script = mint_script(
            "\"$HOME/dispatch-platform/public/live\"",
            "\"$HOME/dispatch-platform/public/config/platform.env\"",
            environment,
            origin,
        );
        let remote = format!("bash --noprofile --norc -c {}", quote(&script));
        runner.command(
            &[
                "ssh",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=10",
                "dispatch-production",
                &remote,
            ],
            None,
            60,
        )
    }
    .map_err(|_| {
        format!(
            "Could not mint {target} browser access. Check the host connection, installed backend and configuration."
        )
    })?;
    require(
        bytes.len() <= 16_384,
        "Invalid browser credential response.",
    )?;
    let credential: Credential =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid browser credential response.")?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let token = credential
        .token
        .strip_prefix("observation-read:")
        .and_then(|rest| rest.split_once(':'));
    let valid_token = token.is_some_and(|(claims, signature)| {
        !claims.is_empty()
            && !signature.is_empty()
            && claims
                .bytes()
                .chain(signature.bytes())
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_=+/".contains(&byte))
    });
    require(
        credential.origin == origin
            && credential.read_only
            && valid_token
            && credential.expires_at > now
            && credential.expires_at <= now + 900_000 + 30_000,
        "The browser credential has an unexpected origin, access mode or expiry.",
    )?;
    // Recheck the destination after the mint; keep the new file at mode 0600 and atomically
    // install it without clobbering a destination created in the meantime.
    output_parent(output)?;
    let mut file = tempfile::Builder::new()
        .prefix(".dispatch-browser-")
        .tempfile_in(parent)?;
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.persist_noclobber(output)
        .map_err(|_| "Could not save private browser credentials without replacing a file.")?;
    Ok(json!({
        "origin": credential.origin,
        "expiresAt": credential.expires_at,
        "readOnly": true,
    }))
}
