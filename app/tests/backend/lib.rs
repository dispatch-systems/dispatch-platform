#[test]
fn every_collection_has_exactly_one_keeper() {
    crate::install();
    super::REGISTRY.check();
}
