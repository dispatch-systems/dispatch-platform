//! Saving a section: what's sent checked and merged onto what's kept, at the revision it was
//! read at, confirmed when its warning has words, and recorded in the DSP's log.
use super::{kinds, store};
use crate::{
    Error, Result,
    db::{AuditChange, Store},
    ensure,
    manifest::settings::{Scope, Section},
};
use serde_json::{Map, Value};

/// What a save did.
#[derive(Debug, PartialEq)]
pub enum Saved {
    /// Kept, or nothing changed: the section's revision and every value now.
    Saved { revision: i64, values: Value },
    /// Nothing kept: the words to confirm before saving it so.
    Confirm(String),
}

/// Saves `values` of `section` for the DSP, or for a member's own section `actor`'s, as read
/// at `revision`: any of its fields, each the rest keep. Refuses a field it hasn't or a value
/// its kind doesn't hold (`invalid_input`), and a revision another save has passed
/// (`settings_changed_reload_before_saving`). Unless `confirmed`, a save its `warn` has words
/// for keeps nothing and answers them. Then, in one write, it keeps the values, does what the
/// section's `saved` does, and records what changed in the DSP's log, as each field shows it,
/// about the section by its label; a member's own changes aren't recorded. `dsp` is the one
/// the caller was authorized for: only its database is read and written, and only it is
/// handed to the section's own code.
pub fn save(
    db: &Store,
    dsp: &str,
    actor: &str,
    section: &Section,
    values: &Value,
    revision: i64,
    confirmed: bool,
) -> Result<Saved> {
    let given = values
        .as_object()
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let member = (section.scope == Scope::Member).then_some(actor);
    let record = store::record(db, dsp, section, member)?;
    let now = store::current(db, dsp, section, &record)?;
    ensure(
        now.revision == revision,
        "settings_changed_reload_before_saving",
        409,
    )?;
    let (after, offered) = merged(db, dsp, section, &now.values, given)?;
    let changes: Vec<AuditChange> = section
        .fields
        .iter()
        .filter(|field| after.get(field.id) != now.values.get(field.id))
        .map(|field| {
            let options = offered
                .iter()
                .find(|(id, _)| *id == field.id)
                .and_then(|(_, options)| options.as_deref());
            let shown =
                |values: &Map<String, Value>| kinds::shown(field, &values[field.id], options);
            (field.label, shown(&now.values), shown(&after))
        })
        .collect();
    if changes.is_empty() {
        return Ok(Saved::Saved {
            revision,
            values: Value::Object(now.values),
        });
    }
    let (before, after) = (Value::Object(now.values), Value::Object(after));
    if let Some(warn) = section.warn.filter(|_| !confirmed)
        && let Some(words) = warn(db, dsp, &before, &after)?
    {
        return Ok(Saved::Confirm(words));
    }
    let revision = revision + 1;
    let people = db.dsp(dsp)?;
    db.across(&people, || {
        store::write(&people, section, member, revision, &after, Some(actor))?;
        if let Some(kept) = section.kept {
            kept.keep(db, dsp, actor, &after)?;
        }
        if let Some(saved) = section.saved {
            saved(db, dsp, &after)?;
        }
        if member.is_none() {
            db.audit_ref(
                Some(actor),
                Some(dsp),
                "settings.changed",
                section.label,
                Some(section.label),
                &changes,
                Some(("setting", section.id)),
            )?;
        }
        Ok(())
    })?;
    Ok(Saved::Saved {
        revision,
        values: after,
    })
}

/// Keeps `values` as the DSP's first of `section` when nothing of it was ever saved: what a
/// feature does once, from its `upgrade_storage`, as it moves values it kept itself into its
/// declared section. Checked as a save checks them (`invalid_input` otherwise), kept at
/// revision 1, and not recorded in the log, whose entries from before already say how they
/// came to be. Answers whether it kept them: not when the section was ever saved, or the
/// values are its defaults. Only `dsp`'s own database is read and written.
pub fn adopt(db: &Store, dsp: &str, section: &Section, values: &Value) -> Result<bool> {
    ensure(
        section.scope == Scope::Dsp && section.kept.is_none(),
        "settings_scope",
        500,
    )?;
    let given = values
        .as_object()
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let now = store::current(db, dsp, section, &store::record(db, dsp, section, None)?)?;
    if now.revision != 0 {
        return Ok(false);
    }
    let (after, _) = merged(db, dsp, section, &now.values, given)?;
    if after == now.values {
        return Ok(false);
    }
    store::write(
        &*db.dsp(dsp)?,
        section,
        None,
        1,
        &Value::Object(after),
        None,
    )?;
    Ok(true)
}

/// The options of a field sent, listed once, for checking it and showing its change.
type Offered = Vec<(&'static str, Option<Vec<(String, String)>>)>;
/// `now` with each of the `given` fields' values, each checked as its kind says, and the
/// options of each: refused as `invalid_input` for a field the section hasn't, or a schedules
/// field, which holds no value.
fn merged(
    db: &Store,
    dsp: &str,
    section: &Section,
    now: &Map<String, Value>,
    given: &Map<String, Value>,
) -> Result<(Map<String, Value>, Offered)> {
    let mut after = now.clone();
    let mut offered = Vec::with_capacity(given.len());
    for (id, value) in given {
        let field = section
            .fields
            .iter()
            .find(|field| field.id == id && kinds::default(field).is_some())
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let options = kinds::options(db, dsp, field)?;
        let checked = kinds::checked(field, value, &now[field.id], options.as_deref())?;
        after.insert(field.id.to_owned(), checked);
        offered.push((field.id, options));
    }
    Ok((after, offered))
}
