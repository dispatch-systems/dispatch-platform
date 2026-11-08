//! Each file of a DSP's folder belongs to one thing a backup can hold, and only files a backup
//! can copy whole are held: no link, no file linked from elsewhere, none of SQLite's own beside a
//! database, nothing of `cache`.
use super::*;

#[test]
fn each_file_of_a_dsps_folder_belongs_to_one_item() {
    crate::testing::install(&[], &[]);
    let folder = tempfile::tempdir().unwrap();
    let at = |path: &str| {
        let path = folder.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        path
    };
    for path in [
        "config/settings.json",
        "data/dispatch.sqlite",
        "data/dispatch.sqlite-wal",
        "data/dispatch.sqlite-shm",
        "data/phones/files/a1",
        "secrets/vault.key",
        "secrets/paycom.enc",
        "state/browsers/paycom/Cookies",
        "state/credential-leases/paycom",
        "cache/phones/files/b2",
    ] {
        fs::write(at(path), path).unwrap();
    }
    std::os::unix::fs::symlink(at("secrets/vault.key"), at("state/browsers/link")).unwrap();
    fs::hard_link(at("config/settings.json"), at("config/linked.json")).unwrap();

    let every: Vec<String> = items(folder.path())
        .into_iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(every, [DISPATCH, LOGINS, PICTURES]);
    let found: Vec<(String, String, bool)> = files(folder.path(), &every)
        .unwrap()
        .into_iter()
        .map(|found| {
            (
                found.path.to_string_lossy().into_owned(),
                found.item,
                found.database,
            )
        })
        .collect();
    let expected = [
        ("data/dispatch.sqlite", DISPATCH, true),
        ("data/phones/files/a1", PICTURES, false),
        ("secrets/paycom.enc", LOGINS, false),
        ("secrets/vault.key", LOGINS, false),
        ("state/credential-leases/paycom", DISPATCH, false),
    ];
    assert_eq!(
        found,
        expected.map(|(path, item, database)| (path.to_owned(), item.to_owned(), database))
    );
    // Left out, an item's files aren't held.
    let held: Vec<String> = files(folder.path(), &[DISPATCH.into()])
        .unwrap()
        .into_iter()
        .map(|found| found.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        held,
        ["data/dispatch.sqlite", "state/credential-leases/paycom"]
    );
}

#[test]
fn a_folder_that_cant_be_read_fails_rather_than_drops_out() {
    use std::os::unix::fs::PermissionsExt;
    crate::testing::install(&[], &[]);
    let folder = tempfile::tempdir().unwrap();
    let locked = folder.path().join("data/paycom");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("paycom.sqlite"), "").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let read = files(folder.path(), &[DISPATCH.into(), PICTURES.into()]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(read.is_err());
}
