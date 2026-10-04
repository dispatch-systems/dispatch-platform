//! What features hook into, as it ran before they registered it: the scheduler's upkeep
//! (Routes' retention, then Driver Match's hourly codes), Driver Match after every
//! successful collection, Timecard's demo data and DVIC's operator commands.
use dispatch_core::manifest::registry;

#[cfg(all(feature = "routes", feature = "driver_match"))]
#[test]
fn the_same_upkeep_runs_in_the_same_order_and_as_often() {
    crate::install();
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

#[cfg(all(feature = "timecard", feature = "dvic"))]
#[test]
fn the_same_features_seed_demo_data_and_answer_commands() {
    crate::install();
    let features = || registry().features.iter();
    let demo: Vec<_> = features()
        .filter(|feature| feature.demo.is_some())
        .map(|feature| feature.name)
        .collect();
    assert_eq!(demo, ["timecard"]);
    let commands: Vec<_> = features()
        .filter_map(|feature| Some((feature.name, feature.commands.as_ref()?.prefix)))
        .collect();
    assert_eq!(commands, [("dvic", "dvic-")]);
}
