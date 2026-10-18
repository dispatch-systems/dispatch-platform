//! The events slot: what happened for a DSP, which any feature hears of without depending on
//! where it happened, or knowing who hears.
//!
//! An event is a type of its own. Core's are here: a collection succeeded, or the DSP's people
//! changed outside one. A feature's own is in its contract crate, beside what it exposes, so a
//! feature hearing it depends on neither. Whoever an event happens to says so with `publish`; a
//! feature hears it by naming it in its manifest's `on`, with `on!`, and hears it only while
//! it's on for the DSP.
use crate::{
    State,
    foundation::observability,
    manifest::{Feature, Upkeep, registry},
};
use serde_json::json;
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    future::Future,
    sync::Arc,
};

/// Something that happened for a DSP, as whoever publishes it says it.
pub trait Event: Any + Clone + Send + Sync {
    /// What it's known by, as `dispatchdev list events` lists it: `collection.succeeded`.
    const NAME: &'static str;
}

/// What a feature does when an event happens for a DSP.
pub struct On {
    /// The event's name, as it says it.
    pub event: &'static str,
    /// The event's type, as Rust names it: `dispatch_core::manifest::events::PeopleChanged`.
    pub kind: fn() -> &'static str,
    /// Whether what happened is its event.
    pub hears: fn(&(dyn Any + Send + Sync)) -> bool,
    /// The work it does for the DSP named, once `hears` says it's its event.
    pub run: fn(Arc<State>, String, &(dyn Any + Send + Sync)) -> Upkeep,
}

/// What a feature does when `$event` happens for a DSP: `$run(state, dsp, event)`, returning
/// its `Upkeep`. `on!(PeopleChanged, backend::told)`.
#[macro_export]
macro_rules! on {
    ($event:ty, $run:path $(,)?) => {
        $crate::manifest::events::On {
            event: <$event as $crate::manifest::events::Event>::NAME,
            kind: ::std::any::type_name::<$event>,
            hears: |event| event.is::<$event>(),
            run: |state, dsp, event| {
                let event = event
                    .downcast_ref::<$event>()
                    .expect("an event is run only once it's heard as its own");
                $run(state, dsp, ::std::clone::Clone::clone(event))
            },
        }
    };
}

/// A collection succeeded for the DSP, its outcome recorded, before the dashboard hears of it.
#[derive(Clone, Debug)]
pub struct CollectionSucceeded {
    /// The kind of data it collected, as its keeper keeps it: `dvic`.
    pub keeps: &'static str,
}
impl Event for CollectionSucceeded {
    const NAME: &'static str = "collection.succeeded";
}

/// The people a DSP's features name changed outside a collection, as when a feature reads new
/// accounts or a feature naming people is switched on: they're told apart again at once.
#[derive(Clone, Debug)]
pub struct PeopleChanged;
impl Event for PeopleChanged {
    const NAME: &'static str = "people.changed";
}

/// Tells every feature on for the DSP that hears `event` of it, one at a time, each in the
/// registry's order, once the one before has done its work.
pub async fn publish<E: Event>(state: &Arc<State>, dsp: &str, event: E) {
    let is_on = |feature| on_for(state, dsp, feature, E::NAME);
    deliver(registry().features, state, dsp, &event, is_on).await;
}

/// Whether `feature` is on for the DSP, as the event `event` reaches it: a mandatory one always,
/// with no switch to read, and one whose switch can't be read not, which is logged.
async fn on_for(state: &Arc<State>, dsp: &str, feature: &'static Feature, event: &str) -> bool {
    if feature.switch.mandatory {
        return true;
    }
    let (switch, at) = (feature.switch.id, dsp.to_owned());
    let read = state.read(move |db| db.feature_enabled(&at, switch)).await;
    read.unwrap_or_else(|error| {
        observability::event(
            "error",
            "events.unheard",
            json!({"event":event,"feature":feature.name,"error":error.code}),
        );
        false
    })
}

/// Tells each of `features` that hears `event` of it, in their order, while `is_on` says it's
/// on for the DSP: one that's off isn't run at all.
async fn deliver<E: Event, On: Future<Output = bool>>(
    features: &[&'static Feature],
    state: &Arc<State>,
    dsp: &str,
    event: &E,
    is_on: impl Fn(&'static Feature) -> On,
) {
    for feature in features {
        for it in feature.on {
            if (it.hears)(event) && is_on(feature).await {
                (it.run)(state.clone(), dsp.to_owned(), event).await;
            }
        }
    }
}

/// Panics unless each event's name is one type's, and no feature hears one twice.
pub(super) fn check(features: &[&'static Feature]) {
    let mut kinds = BTreeMap::<&str, &str>::new();
    for feature in features {
        let mut heard = BTreeSet::new();
        for on in feature.on {
            let kind = (on.kind)();
            let named = *kinds.entry(on.event).or_insert(kind);
            assert!(
                named == kind,
                "{named} and {kind} are each called {}; an event's name is its own",
                on.event
            );
            assert!(
                heard.insert(on.event),
                "{} hears {} twice; once does all it does",
                feature.name,
                on.event
            );
        }
    }
}

#[cfg(test)]
#[path = "../tests/backend/events.rs"]
mod tests;
