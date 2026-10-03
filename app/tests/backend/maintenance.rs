//! The scheduler's upkeep and the pass after each collection, as they ran before features
//! registered them: Routes' retention, then Driver Match's hourly codes, and Driver Match
//! after every successful collection.
use crate::manifest::registry;

#[test]
fn the_same_upkeep_runs_in_the_same_order_and_as_often() {
    let upkeep: Vec<_> = registry()
        .features
        .iter()
        .flat_map(|feature| {
            feature
                .maintenance
                .iter()
                .map(|task| (feature.name, task.every.as_secs()))
        })
        .collect();
    assert_eq!(upkeep, [("routes", 3600), ("driver_match", 3600)]);
    let after: Vec<_> = registry()
        .features
        .iter()
        .filter(|feature| feature.after_collection.is_some())
        .map(|feature| feature.name)
        .collect();
    assert_eq!(after, ["driver_match"]);
}
