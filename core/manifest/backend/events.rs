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
    /// The work it does for the DSP named, if what happened is its event: none for another.
    pub run: fn(Arc<State>, String, &(dyn Any + Send + Sync)) -> Option<Upkeep>,
}

/// What a feature does when `$event` happens for a DSP: `$run(state, dsp, event)`, returning
/// its `Upkeep`. `on!(PeopleChanged, backend::told)`.
#[macro_export]
macro_rules! on {
    ($event:ty, $run:path $(,)?) => {
        $crate::manifest::events::On {
            event: <$event as $crate::manifest::events::Event>::NAME,
            kind: ::std::any::type_name::<$event>,
            run: |state, dsp, event| {
                event
                    .downcast_ref::<$event>()
                    .map(|event| $run(state, dsp, ::std::clone::Clone::clone(event)))
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
    for feature in registry().features {
        for on in feature.on {
            let Some(work) = (on.run)(state.clone(), dsp.to_owned(), &event) else {
                continue;
            };
            let (at, switch) = (dsp.to_owned(), feature.switch.id);
            match state.read(move |db| db.feature_enabled(&at, switch)).await {
                Ok(true) => work.await,
                Ok(false) => {}
                Err(error) => observability::event(
                    "error",
                    "events.unheard",
                    json!({"dspId":dsp,"event":E::NAME,"feature":feature.name,"error":error.code}),
                ),
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
