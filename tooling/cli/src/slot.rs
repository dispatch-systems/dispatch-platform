//! Slots: at most two of dispatchdev's heavy build steps run at once on this machine, from any
//! worktree, and one test run's tests, and another waits its turn, saying once what it waits
//! for. Side by side, builds slow each other down: `dispatchdev check` takes under a minute
//! alone, and ten with three others building; tests that share a loaded machine fail on their
//! time limits. A slot is a lock on a file in the repository's shared Git folder, which the
//! system lets go of when its process ends, however it ends. A step holding one passes it on to
//! the commands it starts, so a `dispatchdev build` inside it never waits for its own step. A
//! step that needs both takes the test slot first, so no two steps each hold one the other
//! waits for. CI's runners take none.
use crate::{Result, Runner, build::Environment, workspace};
use fs2::FileExt;
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Instant,
};

/// A kind of slot: what it is for, how many there are, their folder in the repository's shared
/// Git folder, and what a step holding one passes to the commands it starts: its slot's file.
pub struct Kind {
    name: &'static str,
    slots: usize,
    folder: &'static str,
    held: &'static str,
}
/// Heavy build steps, two at once on this machine's four cores.
pub const BUILD: Kind = Kind {
    name: "build",
    slots: 2,
    folder: "dispatch-build-slots",
    held: "DISPATCH_BUILD_SLOT",
};
/// Steps that run tests, one at a time.
pub const TEST: Kind = Kind {
    name: "test",
    slots: 1,
    folder: "dispatch-test-slots",
    held: "DISPATCH_TEST_SLOT",
};
impl Kind {
    /// How a wait for one is told, so a run's can be read from its log.
    pub fn waiting(&self) -> String {
        format!("Waiting for a {} slot", self.name)
    }
}

/// What a step passes on to the commands it starts: `TMPDIR` set to `tmp`, and the slots it
/// holds.
pub fn passed_on<'a>(
    tmp: &'a Path,
    slots: impl IntoIterator<Item = &'a Slot>,
) -> Vec<(&'a str, &'a OsStr)> {
    let mut env = vec![("TMPDIR", tmp.as_os_str())];
    for slot in slots {
        env.push((slot.held, slot.path.as_os_str()));
    }
    env
}
/// A slot, held until it drops.
pub struct Slot {
    /// Its file, which its kind's `held` names to the commands the step starts.
    pub path: PathBuf,
    held: &'static str,
    _lock: File,
}
impl Slot {
    /// A build slot for `what`, a command in the checkout `root`, as `of` takes one.
    pub fn take(
        root: &Path,
        what: &str,
        env: &Environment,
        runner: &dyn Runner,
    ) -> Result<Option<Self>> {
        Self::of(&BUILD, root, what, env, runner)
    }
    /// The test slot for `what`, as `of` takes one. A step that also builds takes it before
    /// its build slot.
    pub fn test(
        root: &Path,
        what: &str,
        env: &Environment,
        runner: &dyn Runner,
    ) -> Result<Option<Self>> {
        Self::of(&TEST, root, what, env, runner)
    }
    /// A slot of `kind` for `what`, a command in the checkout `root`: a free one, or the first
    /// to come free. None where none is needed: on CI, or in a command a step holding one
    /// started.
    fn of(
        kind: &Kind,
        root: &Path,
        what: &str,
        env: &Environment,
        runner: &dyn Runner,
    ) -> Result<Option<Self>> {
        if env.get("CI").is_some_and(|v| v == "true") || env.contains_key(kind.held) {
            return Ok(None);
        }
        let common = workspace::shared_git(root, runner)?;
        let worktree = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("a checkout");
        let slot = Self::wait(
            kind,
            &common,
            &format!("{what} in {worktree}"),
            &|seconds| std::thread::sleep(std::time::Duration::from_secs(seconds)),
            &mut |text| println!("{text}"),
        )?;
        Ok(Some(slot))
    }
    /// One of `kind`'s slots in the shared Git folder `common`, held by `holder`, as those
    /// waiting read it. While none is free it says once what holds them, and tries again after
    /// each `pause`; once it has one, how long it waited.
    fn wait(
        kind: &Kind,
        common: &Path,
        holder: &str,
        pause: &dyn Fn(u64),
        say: &mut dyn FnMut(&str),
    ) -> Result<Self> {
        let folder = common.join(kind.folder);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&folder)?;
        let started = Instant::now();
        let mut waited = false;
        loop {
            let mut holders = vec![];
            for slot in 0..kind.slots {
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
                                "Got a {} slot after {}.",
                                kind.name,
                                workspace::duration(started.elapsed())
                            ));
                        }
                        return Ok(Self {
                            path,
                            held: kind.held,
                            _lock: file,
                        });
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
                say(&format!(
                    "{}, held by {}.",
                    kind.waiting(),
                    holders.join(" and by ")
                ));
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
            Slot::wait(&BUILD, folder.path(), holder, pause, &mut |text| {
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
        assert_eq!(third.path, folder.path().join(BUILD.folder).join("1"));
        // A slot is let go of when it drops, as when its process ends. A process another test
        // starts meanwhile holds a copy of the lock until it runs its command, so the slot can
        // take a moment to come free.
        drop(first);
        let tries = std::cell::Cell::new(0);
        let pause = |_| {
            tries.set(tries.get() + 1);
            assert!(tries.get() < 5000, "a dropped slot is never let go of");
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        let fourth = take("cargo build in four", &pause, &mut vec![]);
        assert_eq!(fourth.path, folder.path().join(BUILD.folder).join("0"));
    }
    #[test]
    fn one_run_tests_at_a_time_and_another_waits_for_it_while_builds_go_on() {
        let folder = tempfile::tempdir().unwrap();
        let free: &dyn Fn(u64) = &|_| panic!("a free slot is taken without waiting");
        let mut said = vec![];
        let mut say = |text: &str| said.push(text.to_owned());
        let first = Slot::wait(
            &TEST,
            folder.path(),
            "npm run test:ui in one",
            free,
            &mut say,
        );
        let first = RefCell::new(Some(first.unwrap()));
        // Another run builds meanwhile, in a build slot, then waits to run its tests.
        let build = Slot::wait(&BUILD, folder.path(), "cargo test in two", free, &mut say);
        let second = Slot::wait(
            &TEST,
            folder.path(),
            "cargo test in two",
            &|_| drop(first.borrow_mut().take()),
            &mut say,
        );
        assert_eq!(
            said,
            [
                "Waiting for a test slot, held by npm run test:ui in one.",
                "Got a test slot after 0s.",
            ]
        );
        assert!(build.is_ok() && second.is_ok());
    }
    #[test]
    fn a_command_a_step_holding_a_slot_started_takes_none_and_nor_does_ci() {
        struct Unused;
        impl Runner for Unused {
            fn command(&self, _: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                panic!("no slot is looked for")
            }
        }
        type Take = fn(&Path, &str, &Environment, &dyn Runner) -> Result<Option<Slot>>;
        let kinds: [(Take, &str); 2] = [(Slot::take, BUILD.held), (Slot::test, TEST.held)];
        for (take, held) in kinds {
            for (name, value) in [(held, "/repo/.git/dispatch-slots/0"), ("CI", "true")] {
                let env = Environment::from([(name.to_owned(), value.to_owned())]);
                let slot = take(Path::new("/repo"), "cargo test", &env, &Unused).unwrap();
                assert!(slot.is_none(), "{name}");
            }
        }
    }
}
