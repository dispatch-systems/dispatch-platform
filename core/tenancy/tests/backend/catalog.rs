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
