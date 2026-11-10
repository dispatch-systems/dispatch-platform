//! A test run's build folder, handed from one run to the next through the real entry point.
//! These tests have a process of their own, one at a time: a run holds its folder by a lock on
//! an open file, which a child another thread starts shares from its start until it becomes its
//! own program, so beside tests that start commands a folder a run just let go of still looks
//! held, and the next run builds elsewhere.
use dispatchdev_cli::{test::run_commands, workspace};
use fs2::FileExt;
use std::{
    fs,
    sync::{Mutex, MutexGuard},
};

/// Held by the one test running.
fn alone() -> MutexGuard<'static, ()> {
    static ONE: Mutex<()> = Mutex::new(());
    ONE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn a_run_builds_where_the_last_did_and_deletes_it_unless_kept() {
    let _alone = alone();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("run-test-checkout");
    fs::create_dir_all(&root).unwrap();
    let command = |text: &str| vec![text.to_owned()];
    // Each command sees its run's folder and no incremental builds; the folder goes after.
    let wrote =
        "echo \"$CARGO_TARGET_DIR $CARGO_INCREMENTAL\" > seen && touch \"$CARGO_TARGET_DIR/built\"";
    let first = root.join(".test-build").join("1");
    assert!(run_commands(&root, &command(wrote), false).unwrap());
    let seen = fs::read_to_string(root.join("seen")).unwrap();
    assert_eq!(seen.trim(), format!("{} 0", first.display()));
    assert!(!first.exists(), "the run's build is deleted");
    // A failure stops the run, and still deletes what it built.
    assert!(
        !run_commands(
            &root,
            &[wrote.to_owned(), "exit 1".into(), "touch never".into()],
            false
        )
        .unwrap()
    );
    assert!(!root.join("never").exists() && !first.exists());
    // Kept, it stays until the next run, which builds there again from nothing; a folder a
    // killed run left goes then too.
    assert!(run_commands(&root, &command(wrote), true).unwrap());
    assert!(first.join("built").exists());
    let dead = root.join(".test-build").join("2");
    fs::create_dir_all(&dead).unwrap();
    let fresh = "test ! -e \"$CARGO_TARGET_DIR/built\" && echo \"$CARGO_TARGET_DIR\" > seen";
    assert!(run_commands(&root, &command(fresh), true).unwrap());
    assert_eq!(
        fs::read_to_string(root.join("seen")).unwrap().trim(),
        first.to_str().unwrap()
    );
    assert!(!dead.exists());
    assert!(run_commands(&root, &command("true"), false).unwrap());
    assert!(!first.exists());
    let _ = fs::remove_dir_all(workspace::scratch("run-test-checkout"));
}
#[test]
fn a_run_leaves_a_folder_it_cannot_hold_and_builds_in_the_next() {
    let _alone = alone();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("held-test-checkout");
    let first = root.join(".test-build").join("1");
    fs::create_dir_all(&first).unwrap();
    fs::write(first.join("active"), "keep").unwrap();
    // Another run holds it, by the lock a run takes on its folder.
    let other = fs::File::open(&first).unwrap();
    other.try_lock_exclusive().expect("no run holds it yet");
    let wrote = ["echo \"$CARGO_TARGET_DIR\" > seen".to_owned()];
    assert!(run_commands(&root, &wrote, false).unwrap());
    assert_eq!(
        fs::read_to_string(root.join("seen")).unwrap().trim(),
        root.join(".test-build").join("2").to_str().unwrap()
    );
    assert!(first.join("active").exists(), "another run holds it");
    // Once that run ended without deleting it, the next takes it, emptied.
    drop(other);
    assert!(run_commands(&root, &["true".into()], false).unwrap());
    assert!(!first.exists());
    // A link a commit carried there is never followed: not the folder's, nor the folders'.
    let outside = temp.path().join("outside");
    fs::create_dir_all(outside.join("1")).unwrap();
    fs::write(outside.join("1/mine"), "keep").unwrap();
    std::os::unix::fs::symlink(outside.join("1"), &first).unwrap();
    assert!(run_commands(&root, &wrote, false).unwrap());
    assert_eq!(
        fs::read_to_string(root.join("seen")).unwrap().trim(),
        root.join(".test-build").join("2").to_str().unwrap()
    );
    fs::remove_dir_all(root.join(".test-build")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join(".test-build")).unwrap();
    let refused = run_commands(&root, &["true".into()], false).unwrap_err();
    assert!(refused.to_string().contains("is a link"), "{refused}");
    assert!(outside.join("1/mine").exists());
    let _ = fs::remove_dir_all(workspace::scratch("held-test-checkout"));
}
