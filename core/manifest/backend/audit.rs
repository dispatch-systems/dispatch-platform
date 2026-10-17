//! The audit slot: how a feature's actions read in the activity log, by what they begin with.
use crate::{db::Store, manifest::Registry, tenancy::api::audit::AuditArea};
use serde_json::Value;

/// How a feature's actions read in the activity log.
pub struct Audit {
    /// What its actions begin with, each with the area of the log they are listed and
    /// counted under. The log lists the rest under settings.
    pub areas: &'static [(&'static str, AuditArea)],
    /// The kinds of record its events' references name, beside core's.
    pub subjects: &'static [&'static str],
    /// Puts names to what the events the log lists name, as they are today.
    pub names: Option<fn(&Store, &mut [Value])>,
}
impl Audit {
    pub const NONE: Self = Self {
        areas: &[],
        subjects: &[],
        names: None,
    };
}

/// Panics unless every audit prefix is a dotted name listed under an area other than settings.
pub(crate) fn check(registry: &Registry) {
    // Prefixes are written into the log's SQL.
    let agents = registry.agents.into_iter().flat_map(|a| a.audit.areas);
    for (prefix, area) in agents.chain(registry.features.iter().flat_map(|f| f.audit.areas)) {
        let plain = prefix.ends_with('.')
            && prefix
                .split('.')
                .all(|part| part.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
        assert!(
            plain && *area != AuditArea::Settings,
            "{prefix} is no audit prefix of an area"
        );
    }
}
