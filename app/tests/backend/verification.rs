//! The permissions whose writes ask a member to have verified who they are recently, as
//! they stood before each permission's owner declared it.
use crate::{http::needs_recent_verification, roles};

#[test]
fn the_same_writes_ask_for_recent_verification() {
    let asking: Vec<_> = roles::PERMISSIONS
        .iter()
        .copied()
        .filter(|permission| needs_recent_verification(permission))
        .collect();
    assert_eq!(
        asking,
        ["connections.manage", "members.invite", "members.manage"]
    );
    // A route open to any of several permissions asks for none.
    assert!(!needs_recent_verification(
        "members.invite|members.manage|roles.manage"
    ));
    assert!(!needs_recent_verification("members.invite|members.manage"));
}
