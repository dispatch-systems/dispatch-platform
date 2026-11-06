//! Per-DSP switch state, startup carryover, and the audited switch command.
pub mod plan;
use super::{
    audit::AuditChange,
    catalog::{Feature, Kind, catalog, effective, find, reads_live},
};
use crate::{
    Result,
    db::{Store, iso},
    ensure,
};
use rusqlite::params;
use std::collections::BTreeMap;

/// Effective features after a switch, with the ordered transitions that were persisted.
#[derive(Debug)]
pub struct SwitchedFeatures {
    pub features: Vec<String>,
    pub changed: Vec<(&'static Feature, bool)>,
}

impl Store {
    /// The DSP's features, in catalog order: what is switched on, less the tabs of a
    /// page that is off.
    pub fn features(&self, dsp: &str) -> Result<Vec<String>> {
        Ok(effective(&self.switches(dsp)?))
    }
    /// Every feature switched on, in catalog order: the mandatory ones whatever is stored. A
    /// feature without a row of its own is at its default, which is how a DSP made before the
    /// feature existed reads.
    pub(super) fn switches(&self, dsp: &str) -> Result<Vec<String>> {
        let stored: BTreeMap<String, bool> = self
            .platform
            .query_as(
                "SELECT feature,enabled FROM dsp_features WHERE dsp_id=?",
                [dsp],
            )?
            .into_iter()
            .collect();
        Ok(catalog()
            .iter()
            .filter(|f| f.mandatory || stored.get(f.id).copied().unwrap_or(f.default))
            .map(|f| f.id.to_owned())
            .collect())
    }
    /// Keeps what every DSP had: a feature or part that was mandatory when the server last
    /// started, and is optional now, is switched on, once, for every DSP there is, so making
    /// it optional takes it from none of them; a DSP made later starts with it off, as with
    /// any optional feature. A page new since the last start that gathers parts DSPs had on
    /// before, as when a page becomes another's part, or that grew from another feature, is
    /// switched on, once, for each DSP with one of those on, so they keep it. Then records
    /// what is mandatory now.
    pub fn keep_mandatory_features(&self) -> Result<()> {
        self.platform.transaction(|| {
            let recorded: BTreeMap<String, bool> = self
                .platform
                .query_as("SELECT feature,mandatory FROM feature_availability", [])?
                .into_iter()
                .collect();
            // A platform starting for the first time has nothing to keep.
            let new_pages = catalog()
                .iter()
                .filter(|f| f.kind == Kind::Page && !recorded.is_empty())
                .filter(|f| !recorded.contains_key(f.id));
            for page in new_pages {
                let parts: Vec<&str> = catalog()
                    .iter()
                    .filter(|f| f.kind == Kind::Sub(page.id))
                    .map(|f| f.id)
                    .chain(page.starts_on_with.iter().copied())
                    .collect();
                if parts.is_empty() {
                    continue;
                }
                self.platform.exec(
                    "INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) \
                     SELECT DISTINCT dsp_id,?1,1,?2 FROM dsp_features WHERE enabled=1 \
                     AND feature IN (SELECT value FROM json_each(?3)) \
                     ON CONFLICT(dsp_id,feature) DO NOTHING",
                    params![page.id, iso(), serde_json::to_string(&parts)?],
                )?;
            }
            let made_optional: Vec<&Feature> = catalog()
                .iter()
                .filter(|f| !f.mandatory && recorded.get(f.id) == Some(&true))
                .collect();
            if !made_optional.is_empty() {
                for row in self.platform.all("SELECT id FROM dsps", [])? {
                    for feature in &made_optional {
                        self.platform.exec(
                            "INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) \
                             VALUES (?,?,1,?) ON CONFLICT(dsp_id,feature) DO UPDATE SET \
                             enabled=1,changed_by=NULL,changed_at=excluded.changed_at",
                            params![crate::db::s(&row, "id"), feature.id, iso()],
                        )?;
                    }
                }
            }
            for feature in catalog().iter().filter(|f| f.kind != Kind::Connection) {
                self.platform.exec(
                    "INSERT INTO feature_availability(feature,mandatory) VALUES (?,?) \
                     ON CONFLICT(feature) DO UPDATE SET mandatory=excluded.mandatory",
                    params![feature.id, feature.mandatory],
                )?;
            }
            Ok(())
        })
    }
    pub fn feature_enabled(&self, dsp: &str, id: &str) -> Result<bool> {
        Ok(self.features(dsp)?.iter().any(|f| f == id))
    }
    /// When the feature `id` came on for the DSP, in milliseconds, as its switch's record says:
    /// none while the DSP doesn't have it, or has it without a record of being switched on, as a
    /// mandatory feature. Hiding or showing it records a later time, so this is never before it
    /// came on: what a feature mustn't keep from before then, it keeps nothing of from before
    /// this.
    pub fn feature_on_since(&self, dsp: &str, id: &str) -> Result<Option<i64>> {
        if !self.feature_enabled(dsp, id)? {
            return Ok(None);
        }
        let found: Option<(String,)> = self.platform.one_as(
            "SELECT changed_at FROM dsp_features WHERE dsp_id=? AND feature=? AND enabled=1",
            [dsp, id],
        )?;
        Ok(found.and_then(|(at,)| {
            chrono::DateTime::parse_from_rfc3339(&at)
                .ok()
                .map(|at| at.timestamp_millis())
        }))
    }
    /// Switches every feature on for a development or preview DSP, so its demo shows every
    /// page, but those that read a provider live (`live_readers`): nothing reads a provider for
    /// it unasked, as no schedule is made for it, until a preview or a test switches one on.
    /// Real DSPs start with none and get theirs from the platform owner.
    pub fn enable_all_features(&self, dsp: &str) -> Result<()> {
        self.write_features(dsp, |f| !reads_live(f), true)
    }
    /// Writes a new DSP's rows, so a later change of a default leaves it as it was made.
    pub fn seed_features(&self, dsp: &str) -> Result<()> {
        self.write_features(dsp, |f| f.default, false)
    }
    /// A row for every feature, `on` or not; `overwrite` replaces rows the DSP has.
    fn write_features(&self, dsp: &str, on: fn(&Feature) -> bool, overwrite: bool) -> Result<()> {
        let sql = if overwrite {
            "INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES (?,?,?,?) \
             ON CONFLICT(dsp_id,feature) DO UPDATE SET enabled=excluded.enabled,changed_at=excluded.changed_at"
        } else {
            "INSERT OR IGNORE INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES (?,?,?,?)"
        };
        for feature in catalog() {
            self.platform
                .exec(sql, params![dsp, feature.id, on(feature), iso()])?;
        }
        Ok(())
    }
    /// Switches one optional feature, and with it whatever depends on it: enabling a page
    /// or a part enables a provider of each capability it, or a page's parts on with it,
    /// lacks, and a page's tabs when none is on; enabling a provider switches off another of
    /// the same capability; disabling a provider disables the pages and parts left without
    /// one; a page left with none of its tabs on is disabled. A page switched off keeps its
    /// parts' switches as they are, so they come back as they were. A mandatory feature or
    /// part has no switch. Every switch is audited; the answer lists them.
    pub fn set_feature(
        &self,
        dsp: &str,
        id: &str,
        enabled: bool,
        actor: &str,
    ) -> Result<SwitchedFeatures> {
        let feature = find(id).ok_or_else(|| crate::Error::new("feature_not_found", 404))?;
        ensure(!feature.mandatory, "feature_mandatory", 409)?;
        self.platform.transaction(|| {
            self.find_dsp(dsp)?;
            let plan = plan::plan(self.switches(dsp)?, feature, enabled)?;
            let changed = plan.changed;
            for (f, on) in &changed {
                self.platform.exec(
                    "INSERT INTO dsp_features(dsp_id,feature,enabled,changed_by,changed_at) \
                     VALUES (?1,?2,?3,?4,?5) ON CONFLICT(dsp_id,feature) DO UPDATE SET \
                     enabled=?3,changed_by=?4,changed_at=?5",
                    params![dsp, f.id, on, actor, iso()],
                )?;
                let cause: Vec<AuditChange> = if f.id == id {
                    vec![]
                } else {
                    vec![("cause", None, Some(feature.name()))]
                };
                self.audit_with(
                    Some(actor),
                    Some(dsp),
                    if *on {
                        "dsp.feature_enabled"
                    } else {
                        "dsp.feature_disabled"
                    },
                    f.id,
                    Some(&f.name()),
                    &cause,
                )?;
            }
            if !changed.is_empty() {
                // Open views sign the DSP revision, so members pick up the change.
                self.platform
                    .exec("UPDATE dsps SET revision=revision+1 WHERE id=?", [dsp])?;
                self.readers_moved();
            }
            Ok(SwitchedFeatures {
                features: self.features(dsp)?,
                changed,
            })
        })
    }
}
