//! An event reaches each feature that hears it while it's on for the DSP, in order; neither one
//! that's off, which isn't run at all, nor one hearing another.
use super::*;
use crate::{manifest::feature, testing};
use std::sync::Mutex;

/// What the stand-ins heard, as `<feature> <dsp>`.
static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn heard(by: &'static str, dsp: String) -> Upkeep {
    Box::pin(async move { HEARD.lock().unwrap().push(format!("{by} {dsp}")) })
}
fn first(_: Arc<State>, dsp: String, _: PeopleChanged) -> Upkeep {
    heard("first", dsp)
}
fn off(_: Arc<State>, _: String, _: PeopleChanged) -> Upkeep {
    panic!("a feature that's off isn't run at all")
}
fn collected(_: Arc<State>, dsp: String, event: CollectionSucceeded) -> Upkeep {
    assert_eq!(event.keeps, "dvic");
    heard("collected", dsp)
}
fn last(_: Arc<State>, dsp: String, _: PeopleChanged) -> Upkeep {
    heard("last", dsp)
}

static FIRST: Feature = Feature {
    on: &[crate::on!(PeopleChanged, first)],
    ..feature("first")
};
static OFF: Feature = Feature {
    on: &[crate::on!(PeopleChanged, off)],
    ..feature("off")
};
static COLLECTED: Feature = Feature {
    on: &[crate::on!(CollectionSucceeded, collected)],
    ..feature("collected")
};
static LAST: Feature = Feature {
    on: &[crate::on!(PeopleChanged, last)],
    ..feature("last")
};

#[tokio::test]
async fn an_event_reaches_each_feature_on_that_hears_it_in_order() {
    testing::install(&[], &[]);
    let (_root, store, dsp) = testing::bootstrapped();
    let state = State::new(store.config.clone()).unwrap();
    let features = [&FIRST, &OFF, &COLLECTED, &LAST];
    let on = |feature: &'static Feature| std::future::ready(feature.name != "off");

    deliver(&features, &state, &dsp, &PeopleChanged, on).await;
    assert_eq!(
        std::mem::take(&mut *HEARD.lock().unwrap()),
        [format!("first {dsp}"), format!("last {dsp}")]
    );

    let collection = CollectionSucceeded { keeps: "dvic" };
    deliver(&features, &state, &dsp, &collection, on).await;
    assert_eq!(*HEARD.lock().unwrap(), [format!("collected {dsp}")]);
}
