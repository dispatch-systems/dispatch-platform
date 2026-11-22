//! The DSP's members, as Driver Match reads them: each by their account, with its name and
//! email address.
use super::*;
use dispatch_core::testing;

#[test]
fn each_member_is_named_by_their_account_with_its_name_and_email() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, store) = testing::seeded();
    let northline = store
        .kept_dsps()
        .unwrap()
        .into_iter()
        .find(|id| store.find_dsp(id).unwrap().name == "Northline Logistics")
        .unwrap();
    let account = store.members(&northline).unwrap()[0].user_id.clone();
    let named = members(&store, &northline).unwrap();
    assert_eq!(named.len(), 1);
    assert_eq!(named[0].id, account);
    assert_eq!(named[0].name, "Jordan Ellis");
    assert_eq!(named[0].emails, ["member@dispatch.test"]);
    assert!(named[0].listed);
}
