//! The permission that lets a member invite, as it stood before its owner declared it.
use dispatch_core::manifest::registry;

#[test]
fn inviting_takes_the_same_permission() {
    crate::install();
    assert_eq!(registry().inviting(), Some("members.invite"));
}
