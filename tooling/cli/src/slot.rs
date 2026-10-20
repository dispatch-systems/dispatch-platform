//! Build slots: at most two of dispatchdev's heavy build steps run at once on this machine, from
//! any worktree, and another waits its turn, saying once what it waits for. Side by side, builds
//! slow each other down: `dispatchdev check` takes under a minute alone, and ten with three
//! others building. A slot is a lock on a file in the repository's shared Git folder, which the
//! system lets go of when its process ends, however it ends. A step holding one passes it on to
//! the commands it starts, so a `dispatchdev build` inside it never waits for its own step. CI's
//! runners take none.
use crate::{Result, Runner, build::Environment, workspace};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Instant,
};

/// How many heavy build steps run at once, on this machine's four cores.
const SLOTS: usize = 2;
/// The slots' folder, in the repository's shared Git folder.
const FOLDER: &str = "dispatch-build-slots";
/// What a step holding a slot passes to the commands it starts: its slot's file.
pub const HELD: &str = "DISPATCH_BUILD_SLOT";
/// How a wait for a slot is told, so a preview's can be read from its log.
pub const WAITING: &str = "Waiting for a build slot";

/// A build slot, held until it drops.
pub struct Slot {
    /// Its file, which `HELD` names to the commands the step starts.
    pub path: PathBuf,
    _lock: File,
}
impl Slot {
    /// A slot for `what`, a command in the checkout `root`: a free one, or the first to come
    /// free. None where none is needed: on CI, or in a command a step holding one started.
    pub fn take(
        root: &Path,
        what: &str,
        env: &Environment,
        runner: &dyn Runner,
    ) -> Result<Option<Self>> {
        if env.get("CI").is_some_and(|v| v == "true") || env.contains_key(HELD) {
            return Ok(None);
        }
        let common = workspace::text(
            runner,
            &[
                "git",
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
            ],
            Some(root),
        )?;
        let worktree = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("a checkout");
        let slot = Self::wait(
            &Path::new(&common).join(FOLDER),
            &format!("{what} in {worktree}"),
            &|seconds| std::thread::sleep(std::time::Duration::from_secs(seconds)),
            &mut |text| println!("{text}"),
        )?;
        Ok(Some(slot))
    }
    /// One of the slots in `folder`, held by `holder`, as those waiting read it. While none is
    /// free it says once what holds them, and tries again after each `pause`; once it has one,
    /// how long it waited.
    fn wait(
        folder: &Path,
        holder: &str,
        pause: &dyn Fn(u64),
        say: &mut dyn FnMut(&str),
    ) -> Result<Self> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)?;
        let started = Instant::now();
        let mut waited = false;
        loop {
            let mut holders = vec![];
            for slot in 0..SLOTS {
                let path = folder.join(slot.to_string());
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&path)?;
                match file.try_lock_exclusive() {
                    Ok(()) => {
                        file.set_len(0)?;
                        file.write_all(holder.as_bytes())?;
                        if waited {
                            say(&format!(
                                "Got a build slot after {}.",
                                workspace::duration(started.elapsed())
                            ));
                        }
                        return Ok(Self { path, _lock: file });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        let mut held = String::new();
                        file.read_to_string(&mut held)?;
                        holders.push(held);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            if !waited {
                say(&format!("{WAITING}, held by {}.", holders.join(" and by ")));
                waited = true;
            }
            pause(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn two_steps_build_at_once_and_a_third_waits_for_one_to_end() {
        let folder = tempfile::tempdir().unwrap();
        let free: &dyn Fn(u64) = &|_| panic!("a free slot is taken without waiting");
        let take = |holder: &str, pause: &dyn Fn(u64), said: &mut Vec<String>| {
            Slot::wait(folder.path(), holder, pause, &mut |text| {
                said.push(text.to_owned())
            })
            .unwrap()
        };
        let mut said = vec![];
        let first = take("cargo test in one", free, &mut said);
        let second = RefCell::new(Some(take("cargo clippy in two", free, &mut said)));
        assert!(said.is_empty());
        // The third says once what it waits for, and takes the slot the first to end lets go of.
        let third = take(
            "cargo build in three",
            &|_| drop(second.borrow_mut().take()),
            &mut said,
        );
        assert_eq!(
            said,
            [
                "Waiting for a build slot, held by cargo test in one and by cargo clippy in two.",
                "Got a build slot after 0s.",
            ]
        );
        assert_eq!(third.path, folder.path().join("1"));
        // A slot is let go of when it drops, as when its process ends.
        drop(first);
        take("cargo build in four", free, &mut vec![]);
    }
    #[test]
    fn a_command_a_step_holding_a_slot_started_takes_none_and_nor_does_ci() {
        struct Unused;
        impl Runner for Unused {
            fn command(&self, _: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                panic!("no slot is looked for")
            }
        }
        for (name, value) in [(HELD, "/repo/.git/dispatch-build-slots/0"), ("CI", "true")] {
            let env = Environment::from([(name.to_owned(), value.to_owned())]);
            let slot = Slot::take(Path::new("/repo"), "cargo build", &env, &Unused).unwrap();
            assert!(slot.is_none(), "{name}");
        }
    }
}
