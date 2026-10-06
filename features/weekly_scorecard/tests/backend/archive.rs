use crate::backend::archive;
use dispatch_core::db::{private_dir, write_private};
use std::{fs, io, path::Path};

fn source(root: &Path) -> std::path::PathBuf {
    let source = private_dir(&root.join("prior")).unwrap();
    write_private(&source.join("scorecard.sqlite"), &vec![42; 70000]).unwrap();
    let nested = private_dir(&source.join("notes")).unwrap();
    write_private(&nested.join("history"), b"preserved history").unwrap();
    source
}

#[test]
fn a_cross_filesystem_archive_preserves_every_file_before_removing_the_source() {
    let root = tempfile::tempdir().unwrap();
    let source = source(root.path());
    let target = root.path().join("archive");
    archive::finish_with(&source, &target, |_, _| {
        Err(io::ErrorKind::CrossesDevices.into())
    })
    .unwrap();
    assert!(!source.exists());
    assert_eq!(
        fs::read(target.join("scorecard.sqlite")).unwrap(),
        vec![42; 70000]
    );
    assert_eq!(
        fs::read(target.join("notes/history")).unwrap(),
        b"preserved history"
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn an_existing_complete_archive_can_finish_an_interrupted_source_removal() {
    let root = tempfile::tempdir().unwrap();
    let source = source(root.path());
    let target = private_dir(&root.path().join("archive")).unwrap();
    fs::copy(
        source.join("scorecard.sqlite"),
        target.join("scorecard.sqlite"),
    )
    .unwrap();
    let notes = private_dir(&target.join("notes")).unwrap();
    fs::copy(source.join("notes/history"), notes.join("history")).unwrap();
    fs::remove_file(source.join("notes/history")).unwrap();
    archive::finish_with(&source, &target, |_, _| {
        panic!("matching archive must be reused")
    })
    .unwrap();
    assert!(!source.exists());
    assert_eq!(
        fs::read(target.join("notes/history")).unwrap(),
        b"preserved history"
    );
}

#[test]
fn a_conflicting_archive_preserves_both_directories() {
    let root = tempfile::tempdir().unwrap();
    let source = source(root.path());
    let target = private_dir(&root.path().join("archive")).unwrap();
    write_private(&target.join("scorecard.sqlite"), &vec![43; 70000]).unwrap();
    let error = archive::finish_with(&source, &target, |_, _| {
        panic!("conflict must stop before moving")
    })
    .unwrap_err();
    assert_eq!(error.code, "weekly_scorecard_migration_backup_conflict");
    assert_eq!(
        fs::read(source.join("scorecard.sqlite")).unwrap(),
        vec![42; 70000]
    );
    assert_eq!(
        fs::read(target.join("scorecard.sqlite")).unwrap(),
        vec![43; 70000]
    );
}
