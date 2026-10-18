//! The exposes slot: what a feature offers every other feature, each through a contract, so a
//! feature uses another's work without depending on it, or knowing which feature does it.
//!
//! A contract is a trait. The feature offering one owns it in a crate of its own beside it,
//! `features/<name>/contract/`, which holds the trait and the types it speaks in and depends on
//! core alone: any feature may use it without declaring the feature, and it stays when the
//! feature is left out of a build, when nothing offers it. A contract core itself uses, as the
//! directory that core's people helpers ask, is core's. A feature offers one in its manifest's
//! `exposes`, with `expose!`; one feature at most offers each, and a feature using it asks for
//! it for a DSP (`exposes::for_dsp::<dyn Files>`), which answers only while the feature
//! offering it is on there.
use crate::{
    Result,
    db::Store,
    manifest::{Feature, Registry},
};
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
};

/// One thing a feature offers, through the contract it fulfils.
pub struct Exposed {
    /// What it's known by, as `dispatchdev list exposes` lists it: `files`, `directory`.
    pub name: &'static str,
    /// What it offers, in a sentence.
    pub about: &'static str,
    /// The contract's type, as `&'static dyn Contract`: a feature asks for it by this.
    pub contract: fn() -> TypeId,
    /// The offering, a `&'static &'static dyn Contract`.
    pub offered: &'static (dyn Any + Send + Sync),
}

/// What a feature offers through the contract `$contract`, a trait: `$offered` fulfils it.
/// `expose!(dyn Files, &backend::FILES, "files", "Somewhere to keep a DSP's files.")`.
#[macro_export]
macro_rules! expose {
    ($contract:ty, $offered:expr, $name:literal, $about:literal $(,)?) => {
        $crate::manifest::exposes::Exposed {
            name: $name,
            about: $about,
            contract: ::std::any::TypeId::of::<&'static $contract>,
            offered: {
                static OFFERED: &'static $contract = $offered;
                &OFFERED
            },
        }
    };
}

impl Registry {
    /// The feature offering the contract `C`, a trait object type such as `dyn Files`, and its
    /// offering, if this build has one.
    pub fn exposed<C: ?Sized + 'static>(&self) -> Option<(&'static Feature, &'static C)> {
        self.features.iter().find_map(|feature| {
            feature
                .exposes
                .iter()
                .find(|exposed| (exposed.contract)() == TypeId::of::<&'static C>())
                .and_then(|exposed| exposed.offered.downcast_ref::<&'static C>())
                .map(|offered| (*feature, *offered))
        })
    }
}

/// The contract `C`'s offering for the DSP `dsp`: only while the feature offering it is on
/// there.
pub fn for_dsp<C: ?Sized + 'static>(db: &Store, dsp: &str) -> Result<Option<&'static C>> {
    let Some((feature, offered)) = crate::manifest::registry().exposed::<C>() else {
        return Ok(None);
    };
    Ok(db
        .feature_enabled(dsp, feature.switch.id)?
        .then_some(offered))
}

/// Panics unless each contract and each name is offered by one feature at most, as the
/// registry is installed.
pub(super) fn check(features: &[&'static Feature]) {
    let mut contracts = BTreeMap::<TypeId, Vec<&str>>::new();
    let mut names = BTreeMap::<&str, Vec<&str>>::new();
    for feature in features {
        for exposed in feature.exposes {
            contracts
                .entry((exposed.contract)())
                .or_default()
                .push(feature.name);
            names.entry(exposed.name).or_default().push(feature.name);
        }
    }
    for (name, by) in names {
        assert!(
            by.len() == 1,
            "{} each expose {name}; one feature at most may",
            by.join(" and ")
        );
    }
    for by in contracts.values() {
        assert!(
            by.len() == 1,
            "{} each offer the same contract; one feature at most may",
            by.join(" and ")
        );
    }
}
