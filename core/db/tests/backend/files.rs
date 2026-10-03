use super::*;
use std::os::unix::fs::PermissionsExt;
#[test]
fn a_file_deleted_during_the_check_is_absent_but_hard_links_stay_unsafe() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.path().join("dispatch.sqlite-wal");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    private_file(&path, false).unwrap();
    let link = root.path().join("link");
    fs::hard_link(&path, &link).unwrap();
    assert!(private_file(&path, false).is_err());
    fs::remove_file(link).unwrap();
    fs::remove_file(&path).unwrap();
    // fstat on the unlinked, still-open inode deterministically supplies the
    // zero-link metadata that lstat can observe during a SQLite sidecar race.
    let deleted = file.metadata().unwrap();
    assert_eq!(deleted.nlink(), 0);
    check_metadata(&path, Ok(deleted)).unwrap();
    private_file(&path, false).unwrap();
    assert!(
        check_metadata(
            &path,
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        )
        .is_err()
    );
}
