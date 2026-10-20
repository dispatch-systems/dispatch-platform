//! Communications's storage, against a real store in a temporary directory.
use super::*;
use dispatch_core::testing;

#[test]
fn keeps_what_it_adds_for_each_dsp() {
    testing::install(&[], &[&crate::FEATURE, &dispatch_slack::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    assert!(store.communications_items(&dsp).unwrap().is_empty());
    let added = store.add_communications_item(&dsp, "First").unwrap();
    assert_eq!(store.communications_items(&dsp).unwrap(), vec![added]);
}
