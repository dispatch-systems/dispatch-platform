//! The preferences slot: what each member arranges for themselves in a DSP, as the order of its
//! sidebar or what their Home page shows. Core keeps each member's own (`accounts::preferences`)
//! a revision at a time, so a save made from an older revision never replaces a newer one. A
//! feature declares each it keeps: what a value must be to be kept, and how a value an earlier
//! version kept reads now. Core's own are declared here too.
use crate::{
    Result,
    manifest::{Feature, Registry},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

/// Core's own preferences, each declared by the core part that keeps it.
const CORE_PREFERENCES: &[&Preference] = &[&crate::accounts::preferences::SIDEBAR];

/// The most bytes any value may take, as JSON.
pub const MOST_BYTES: usize = 64 * 1024;

/// What a preference's value reads as: its shape, which serde holds it to, and `check`. A value
/// is kept and sent as this type writes it, every field it has written out.
pub trait PreferenceValue: DeserializeOwned + Serialize {
    /// Fails unless a value of the right shape is one to keep, as a list within its bounds.
    fn check(&self) -> Result<()> {
        Ok(())
    }
}

/// How a value kept at one version reads at the next; none when it can't.
pub type Upgrade = fn(Value) -> Option<Value>;

/// One of a feature's preferences, as its manifest declares it.
pub struct Preference {
    /// Permanent once shipped, as each member's value is kept under it: `<feature>.<what>`,
    /// a core part's name for core's.
    pub id: &'static str,
    /// How each earlier version of its value reads at the next, the first reading version 1
    /// as 2. Its version is one past the last; a value never changes shape without one.
    pub upgrades: &'static [Upgrade],
    /// The most bytes its value may take, as JSON.
    pub limit: usize,
    /// A value as it is kept, every field written out; none unless it's one it keeps.
    normal: fn(&Value) -> Option<Value>,
}
/// One preference, its value a `T`: `preference::<Layout>("x.layout")`.
pub const fn preference<T: PreferenceValue>(id: &'static str) -> Preference {
    Preference {
        id,
        upgrades: &[],
        limit: 16 * 1024,
        normal: normal::<T>,
    }
}
fn normal<T: PreferenceValue>(value: &Value) -> Option<Value> {
    let value = T::deserialize(value).ok()?;
    value.check().ok()?;
    serde_json::to_value(value).ok()
}

impl Preference {
    /// Its value takes at most `bytes`, as JSON, at most `MOST_BYTES`.
    pub const fn limit(mut self, bytes: usize) -> Self {
        self.limit = bytes;
        self
    }
    /// How the value of each earlier version reads at the next, as its shape changes.
    pub const fn upgrades(mut self, upgrades: &'static [Upgrade]) -> Self {
        self.upgrades = upgrades;
        self
    }
    /// The version its value is kept at now.
    pub fn version(&self) -> u32 {
        self.upgrades.len() as u32 + 1
    }
    /// `value` as it is kept, every field written out: none unless it's within its limit, and
    /// of its shape and bounds.
    pub fn keeps(&self, value: &Value) -> Option<Value> {
        let value = (self.normal)(value)?;
        (value.to_string().len() <= self.limit).then_some(value)
    }
    /// A value kept at `version` as it reads now, upgraded through each version since; none
    /// when it can't be read, as one a newer release kept or one no longer of its shape.
    pub fn read(&self, version: u32, value: Value) -> Option<Value> {
        let since = self.upgrades.get(version.checked_sub(1)? as usize..)?;
        let value = since
            .iter()
            .try_fold(value, |value, upgrade| upgrade(value))?;
        (self.normal)(&value)
    }
}

impl Registry {
    /// Every preference, each with the feature that declares it, none for core's: core's
    /// first, then the features' in the registry's order.
    pub fn preferences(
        &self,
    ) -> impl Iterator<Item = (Option<&'static Feature>, &'static Preference)> {
        CORE_PREFERENCES
            .iter()
            .map(|each| (None, *each))
            .chain(self.features.iter().flat_map(|feature| {
                feature
                    .preferences
                    .iter()
                    .map(move |each| (Some(*feature), each))
            }))
    }
}

/// Panics unless each of the preferences has an id of its own, a feature's under its name,
/// and a limit within `MOST_BYTES`.
pub(crate) fn check(registry: &Registry) {
    let mut seen = std::collections::BTreeSet::new();
    for (feature, each) in registry.preferences() {
        if let Some(feature) = feature {
            assert!(
                each.id.starts_with(&format!("{}.", feature.name)),
                "{} declares {} among its preferences, which is not under its name",
                feature.name,
                each.id
            );
        }
        assert!(
            seen.insert(each.id),
            "{} is declared twice among the preferences",
            each.id
        );
        assert!(
            (1..=MOST_BYTES).contains(&each.limit),
            "{} keeps up to {} bytes, past the {MOST_BYTES} any preference may",
            each.id,
            each.limit
        );
    }
}
