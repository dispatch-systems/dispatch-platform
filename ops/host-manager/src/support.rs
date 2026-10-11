//! Installing AI Support never changes Production. One Dev worker uses the same verified
//! Hermes resource bundle for every configured source platform.
use crate::{
    Result, artifact, io, require,
    updater::{Environment, Updater},
};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::Path};

/// The support sidecar is one versioned bundle, never an unrestricted services directory.
pub(crate) const RESOURCES: &[&str] = &[
    "services/rust/dispatch-support-worker",
    "services/support-worker/dispatch-support-worker.service",
    "services/support-worker/support-collection.py",
    "services/support-worker/hermes/runner.py",
    "services/support-worker/hermes/host-sandbox.py",
    "services/support-worker/hermes/browser.mjs",
    "services/support-worker/hermes/profiles/analyst.json",
    "services/support-worker/hermes/profiles/builder.json",
    "services/support-worker/hermes/profiles/reviewer.json",
    "services/support-worker/hermes/plugin/plugin.yaml",
    "services/support-worker/hermes/plugin/__init__.py",
    "services/support-worker/hermes/skills/browser-investigation/SKILL.md",
    "services/support-worker/hermes/skills/dispatch-context/SKILL.md",
    "services/support-worker/hermes/skills/independent-verification/SKILL.md",
    "services/support-worker/hermes/skills/proposal-analysis/SKILL.md",
    "services/support-worker/hermes/skills/approved-implementation/SKILL.md",
    "services/support-worker/hermes/skills/queue-correction/SKILL.md",
];

pub fn run(args: &[String], system: &dyn io::System) -> Result<()> {
    require(
        args.len() == 3
            && args[0] == "--root"
            && ["--verify", "--install-unit"].contains(&args[2].as_str()),
        "host support --root DEV_ROOT <--verify|--install-unit>",
    )?;
    let updater = Updater::new(Path::new(&args[1]), Environment::Dev, system)?;
    verify(&updater)?;
    if args[2] == "--install-unit" {
        let home = std::env::var_os("HOME").ok_or("Home directory required")?;
        let directory = Path::new(&home).join(".config/systemd/user");
        fs::create_dir_all(&directory)?;
        require(
            directory.canonicalize()? == directory,
            "Unit directory symlink denied",
        )?;
        let source = updater
            .active
            .join("services/support-worker/dispatch-support-worker.service");
        let mut staged = tempfile::NamedTempFile::new_in(&directory)?;
        staged.write_all(&fs::read(&source)?)?;
        staged
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        staged.as_file().sync_all()?;
        staged.persist(directory.join("dispatch-support-worker.service"))?;
        fs::File::open(&directory)?.sync_all()?;
        system.command(&["systemctl", "--user", "daemon-reload"], None, 30, None)?;
        println!(
            "Installed the verified Dev worker unit. Configure its private origins and credential reference before enabling it."
        );
    }
    Ok(())
}

pub fn verify(updater: &Updater<'_>) -> Result<()> {
    require(
        updater.environment == Environment::Dev,
        "AI Support worker runs only on Dev",
    )?;
    let manifest = updater.verify()?;
    require(
        io::read_json(&updater.active.join("tooling/build-info.json"))?["supportWorker"] == 1,
        "Active artifact has no support worker",
    )?;
    for &path in RESOURCES {
        require(
            manifest.files.iter().any(|file| file.path == path),
            "Support resource missing from verified artifact",
        )?;
        require(
            artifact::file_hash(&updater.active.join(path))?
                == manifest
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .expect("checked")
                    .sha256,
            "Support resource changed",
        )?;
    }
    Ok(())
}

/// An inactive worker stays inactive. Restarts recover leases rather than abandoning work.
pub fn refresh(updater: &Updater<'_>) {
    if updater.environment != Environment::Dev
        || !io::read_json(&updater.active.join("tooling/build-info.json"))
            .is_ok_and(|value| value["supportWorker"] == 1)
    {
        return;
    }
    if let Err(error) = updater.system.command(
        &[
            "systemctl",
            "--user",
            "try-restart",
            "dispatch-support-worker.service",
        ],
        None,
        45,
        None,
    ) {
        eprintln!("Support worker restart deferred: {error}");
    }
}
