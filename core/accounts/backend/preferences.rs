//! Each member's own preferences in a DSP (`manifest::preferences`), kept in the DSP's database
//! in `member_preferences`: a row for each member and preference they saved, with the version
//! its value is of and the revision the save made. Nothing is kept until a member saves, so a
//! default changed later reaches everyone who never did, and only they or the deletion of their
//! account clears what they saved.
//!
//! A save names the revision it was made from and is kept only while that is still the one
//! kept: one made from an older one answers what is kept now instead, so a screen open since
//! before another's save makes its change again on that save rather than replacing it. A value
//! this release can't read, as one a newer release kept, is never sent and never replaced by a
//! save; only clearing it does.
use crate::{
    Error, Result,
    accounts::{
        Context,
        api::types::{KeptPreference, PreferenceSaved, SidebarArrangement},
    },
    db::{Store, iso},
    foundation::{observability, validate as v},
    manifest::{Preference, PreferenceValue, preference, registry},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// How the member arranged the app's sidebar: the dashboard knows its pages, so any id is kept.
pub const SIDEBAR: Preference = preference::<SidebarArrangement>("shell.sidebar");

/// The most pages an arrangement names, far past what a DSP has.
const ARRANGED: usize = 200;
impl PreferenceValue for SidebarArrangement {
    fn check(&self) -> Result<()> {
        v::ids(&self.order, ARRANGED)?;
        v::ids(&self.hidden, ARRANGED)
    }
}

/// The preferences a member has in the DSP of `c`: core's, and those of each feature the DSP
/// has and shows them.
pub fn shown(c: &Context) -> impl Iterator<Item = &'static Preference> {
    registry()
        .preferences()
        .filter(|(feature, _)| {
            feature.is_none_or(|feature| {
                feature.belongs_to(c.dsp.code.as_deref()) && c.has(feature.switch.id)
            })
        })
        .map(|(_, preference)| preference)
}

impl Store {
    /// What `user` kept of each of `preferences` in the DSP, by id, in one query: each they
    /// saved, its value none when this release can't read it.
    pub fn preferences<'a>(
        &self,
        dsp: &str,
        user: &str,
        preferences: impl IntoIterator<Item = &'a Preference>,
    ) -> Result<BTreeMap<String, KeptPreference>> {
        let declared: BTreeMap<_, _> = preferences.into_iter().map(|p| (p.id, p)).collect();
        let rows: Vec<(String, u32, i64, String)> = self.dsp(dsp)?.query_as(
            "SELECT id,version,revision,value FROM member_preferences WHERE user_id=?",
            [user],
        )?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, version, revision, value)| {
                let preference = declared.get(id.as_str())?;
                Some((id, kept(preference, version, revision, &value)))
            })
            .collect())
    }

    /// Keeps `value` as `user`'s `preference` in the DSP, or clears it back to its default when
    /// none, if `revision` is the one kept now (0 while nothing is). A value must be one the
    /// preference keeps, and one this release can't read is only cleared.
    pub fn save_preference(
        &self,
        dsp: &str,
        user: &str,
        preference: &Preference,
        revision: i64,
        value: Option<&Value>,
    ) -> Result<PreferenceSaved> {
        if let Some(value) = value {
            crate::ensure(preference.keeps(value), "invalid_input", 400)?;
        }
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let now = db
                .one_as::<(u32, i64, String)>(
                    "SELECT version,revision,value FROM member_preferences \
                     WHERE user_id=? AND id=?",
                    [user, preference.id],
                )?
                .map(|(version, revision, value)| kept(preference, version, revision, &value))
                .unwrap_or(KeptPreference {
                    revision: 0,
                    value: None,
                });
            if now.revision != revision {
                return Ok(PreferenceSaved::Stale { current: now });
            }
            let Some(value) = value else {
                db.exec(
                    "DELETE FROM member_preferences WHERE user_id=? AND id=?",
                    [user, preference.id],
                )?;
                return Ok(PreferenceSaved::Saved { revision: 0 });
            };
            if now.revision > 0 && now.value.is_none() {
                return Err(Error::new("preference_unreadable", 409));
            }
            db.exec(
                "INSERT INTO member_preferences(user_id,id,version,revision,value,saved_at) \
                 VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(user_id,id) DO UPDATE SET \
                 version=excluded.version,revision=excluded.revision,value=excluded.value,\
                 saved_at=excluded.saved_at",
                rusqlite::params![
                    user,
                    preference.id,
                    preference.version(),
                    revision + 1,
                    value.to_string(),
                    iso()
                ],
            )?;
            Ok(PreferenceSaved::Saved {
                revision: revision + 1,
            })
        })
    }
}

/// A row as this release reads it: a value it can't read is logged, and sent as none.
fn kept(preference: &Preference, version: u32, revision: i64, value: &str) -> KeptPreference {
    let value = serde_json::from_str(value)
        .ok()
        .and_then(|value| preference.read(version, value));
    if value.is_none() {
        observability::event(
            "warn",
            "preference.unreadable",
            json!({"preference": preference.id, "version": version}),
        );
    }
    KeptPreference { revision, value }
}
