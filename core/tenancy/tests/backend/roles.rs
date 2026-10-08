//! A role holds everything its permissions grant, however many steps away and in whatever
//! order the features declare them.
use super::*;

#[test]
fn a_role_holds_what_its_permissions_grant_however_far_away() {
    // Declared as a single pass in order would miss: delete grants edit before edit grants
    // view is seen again.
    let implied = [("files.edit", "files.view"), ("files.delete", "files.edit")];
    let mut held = granting_by(&implied, &["files.delete".to_owned()]);
    held.sort();
    assert_eq!(held, ["files.delete", "files.edit", "files.view"]);
}
