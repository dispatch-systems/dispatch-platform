//! The permissions whose writes ask a member to have verified who they are recently, and
//! the one that lets a member invite, as they stood before each permission's owner
//! declared them.
use dispatch_core::{manifest::registry, server::http::needs_recent_verification, tenancy::roles};

#[test]
fn the_same_writes_ask_for_recent_verification() {
    crate::install();
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

#[test]
fn inviting_takes_the_same_permission() {
    crate::install();
    assert_eq!(registry().inviting(), "members.invite");
}
