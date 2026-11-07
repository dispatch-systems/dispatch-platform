use super::environment::Tools;
use crate::{Result, Runner, build::Environment, require};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub(super) struct Device<'a> {
    pub serial: &'a str,
    _lock: File,
}

impl<'a> Device<'a> {
    pub fn take(
        root: &Path,
        serial: &'a str,
        tools: &Tools,
        env: &Environment,
        runner: &dyn Runner,
    ) -> Result<Self> {
        require(
            serial
                .strip_prefix("emulator-")
                .is_some_and(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())),
            "--device must name the emulator opened for this task in T3, such as emulator-5554. Physical phones are not supported.",
        )?;
        let cache = env
            .get("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| env.get("HOME").map(|home| Path::new(home).join(".cache")))
            .ok_or("HOME is required for the emulator operation lock.")?
            .join("dispatchdev/android");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&cache)?;
        let mut lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(cache.join(format!("{serial}.lock")))?;
        if let Err(error) = lock.try_lock_exclusive() {
            if error.kind() != std::io::ErrorKind::WouldBlock {
                return Err(error.into());
            }
            let mut owner = String::new();
            lock.read_to_string(&mut owner)?;
            return Err(format!("{serial} has another dispatchdev operation: {owner}").into());
        }
        lock.set_len(0)?;
        write!(lock, "{} (pid {})", root.display(), std::process::id())?;
        let listed = String::from_utf8(tools.adb(&["devices"], runner)?)?;
        let matches: Vec<_> = listed
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                (fields.next()? == serial).then(|| fields.next().unwrap_or(""))
            })
            .collect();
        require(
            matches == ["device"],
            &format!(
                "{serial} is missing, offline or ambiguous. Open this task's emulator in T3 first."
            ),
        )?;
        let get = |property| -> Result<String> {
            Ok(String::from_utf8(
                tools.adb(&["-s", serial, "shell", "getprop", property], runner)?,
            )?
            .trim()
            .to_owned())
        };
        require(
            get("ro.kernel.qemu")? == "1",
            "The selected target is not an Android emulator.",
        )?;
        require(
            get("sys.boot_completed")? == "1",
            "The selected emulator is still booting. Prepare it after T3 reports it ready.",
        )?;
        Ok(Self {
            serial,
            _lock: lock,
        })
    }

    pub fn install(&self, apk: &Path, tools: &Tools, runner: &dyn Runner) -> Result<()> {
        let output = tools.adb(
            &[
                "-s",
                self.serial,
                "install",
                "-r",
                apk.to_str().ok_or("The APK path is not UTF-8.")?,
            ],
            runner,
        )?;
        require(
            String::from_utf8(output)?
                .lines()
                .any(|line| line.trim() == "Success"),
            "ADB did not confirm installation; the app was not prepared.",
        )
    }
}
