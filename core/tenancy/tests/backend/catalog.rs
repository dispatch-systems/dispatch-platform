//! A permission exists while what owns it is on: a page, or a part of one, such as uploading
//! files, which the page's own permissions don't need.
use super::*;

fn feature(id: &'static str, kind: Kind, permissions: &'static [&'static str]) -> Feature {
    Feature {
        id,
        label: id,
        kind,
        permissions,
        provides: &[],
        requires: &[],
        default: true,
        tab: false,
        mandatory: false,
        connects: false,
    }
}

#[test]
fn a_part_of_a_page_owns_its_permissions_apart_from_the_page() {
    let catalog = [
        feature("notes", Kind::Page, &["notes.view"]),
        feature("notes.uploads", Kind::Sub("notes"), &["notes.upload"]),
    ];
    let on = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
    let page = on(&["notes"]);
    assert!(grants_in(&catalog, &page, "notes.view"));
    assert!(!grants_in(&catalog, &page, "notes.upload"));
    let both = on(&["notes", "notes.uploads"]);
    assert!(grants_in(&catalog, &both, "notes.upload"));
    // A permission no feature owns exists everywhere.
    assert!(grants_in(&catalog, &on(&[]), "members.invite"));
}

// Manage DSP Connections manages the DSP's own accounts, so it exists while a collector's
// connection is on, or a page that adds one of its own, and not for a page that doesn't.
#[test]
fn managing_dsp_connections_exists_while_a_connection_or_a_page_adding_one_is_on() {
    let mut files = feature("files", Kind::Page, &["files.use"]);
    files.connects = true;
    let catalog = [
        feature("payroll", Kind::Connection, &[]),
        files,
        feature("notes", Kind::Page, &["notes.view"]),
    ];
    let on = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
    for (enabled, exists) in [
        (&["payroll"][..], true),
        (&["files"], true),
        (&["notes"], false),
        (&[], false),
    ] {
        assert_eq!(
            grants_in(&catalog, &on(enabled), CONNECTIONS),
            exists,
            "{enabled:?}"
        );
    }
}

// A mandatory feature is never switched on, so a part of it that started on would reach every
// DSP the day it shipped: an optional one starts off, as a new feature does.
#[test]
fn a_mandatory_features_optional_part_starts_off_and_its_mandatory_part_on() {
    static HOST: manifest::Feature = manifest::Feature {
        switch: manifest::mandatory("host", "Host"),
        subfeatures: &[
            manifest::sub("host.extra", "Extra"),
            manifest::tab("host.main", "Main").mandatory(),
        ],
        ..manifest::feature("host")
    };
    let page = page(&HOST);
    assert!(page.mandatory && page.default);
    let parts: Vec<_> = page_subs(&HOST)
        .map(|part| (part.id, part.default, part.mandatory))
        .collect();
    assert_eq!(
        parts,
        [("host.extra", false, false), ("host.main", true, true)]
    );
}
