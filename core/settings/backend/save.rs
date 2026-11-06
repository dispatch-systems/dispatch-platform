//! Section saves and one-time carry-over. Full policy: `core/settings/README.md`.
use super::{kinds, store};
use crate::{
    Error, Result,
    db::{AuditChange, Store},
    ensure,
    foundation::observability,
    manifest::settings::{Scope, Section},
};
use serde_json::{Map, Value, json};

/// What a save did.
#[derive(Debug, PartialEq)]
pub enum Saved {
    /// Kept, or nothing changed: the section's revision and every value now.
    Saved { revision: i64, values: Value },
    /// Nothing kept: the words to confirm before saving it so.
    Confirm(String),
}

/// Partially saves a DSP section, or `actor`'s member section, at `revision`.
/// Refuses invalid fields/values and stale revisions; an unconfirmed warning writes nothing.
/// Values, callbacks and DSP audit commit atomically; member changes aren't audited.
/// The caller must authorize `dsp`: only its database and callbacks are used.
pub fn save(
    db: &Store,
    dsp: &str,
    actor: &str,
    section: &Section,
    values: &Value,
    revision: i64,
    confirmed: bool,
) -> Result<Saved> {
    let (saved, _) = save_listing(db, dsp, actor, section, values, revision, confirmed)?;
    Ok(saved)
}

/// `save`, handing back too the lists the section's fields offer, as it read them to check
/// what's sent before keeping anything: what an answer that shows the section after it lists
/// its options from, rather than reading them again.
pub(crate) fn save_listing<'a>(
    db: &'a Store,
    dsp: &'a str,
    actor: &str,
    section: &Section,
    values: &Value,
    revision: i64,
    confirmed: bool,
) -> Result<(Saved, kinds::Lists<'a>)> {
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
    let mut lists = kinds::Lists::new(db, dsp);
    let after = merged(section, &now.values, given, Some(&mut lists))?;
    let changes = (section.fields.iter())
        .filter(|field| after.get(field.id) != now.values.get(field.id))
        .map(|field| {
            let options = lists.options(field)?;
            let shown = |values: &Map<String, Value>| {
                kinds::shown(field, &values[field.id], options.as_deref())
            };
            Ok((field.label, shown(&now.values), shown(&after)))
        })
        .collect::<Result<Vec<AuditChange>>>()?;
    if changes.is_empty() {
        let values = Value::Object(now.values);
        return Ok((Saved::Saved { revision, values }, lists));
    }
    let (before, after) = (Value::Object(now.values), Value::Object(after));
    if let Some(warn) = section.warn.filter(|_| !confirmed)
        && let Some(words) = warn(db, dsp, &before, &after)?
    {
        return Ok((Saved::Confirm(words), lists));
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
    // A DSP's live reader may read its window from what was saved.
    if member.is_none() {
        db.readers_moved();
    }
    Ok((
        Saved::Saved {
            revision,
            values: after,
        },
        lists,
    ))
}

/// Called by `upgrade_storage` on DSP open, once while `section` has no record.
/// An existing record skips `old`; otherwise each valid field moves and refused fields use
/// defaults. Dynamic option lists are deliberately ignored: old choices may no longer appear.
/// Real moved values use revision 1; nothing beyond defaults writes an empty revision-0 marker
/// so future defaults still apply. Only `dsp`'s database is used, without audit.
/// Failure is nonfatal: log DSP, section and error code without values, then retry next open.
pub fn carry_over(
    db: &Store,
    dsp: &str,
    section: &Section,
    old: impl FnOnce() -> Result<Option<Value>>,
) {
    if let Err(error) = adopt(db, dsp, section, old) {
        observability::event(
            "error",
            "settings.carry_over_failed",
            json!({"dspId": dsp, "section": section.id, "error": error.code}),
        );
    }
}
/// What `carry_over` does, failing as it may.
fn adopt(
    db: &Store,
    dsp: &str,
    section: &Section,
    old: impl FnOnce() -> Result<Option<Value>>,
) -> Result<()> {
    ensure(
        section.scope == Scope::Dsp && section.kept.is_none(),
        "settings_scope",
        500,
    )?;
    let record = store::record(db, dsp, section, None)?;
    if !record.is_null() {
        return Ok(());
    }
    let defaults = store::current(db, dsp, section, &record)?.values;
    let mut moved = defaults.clone();
    if let Some(Value::Object(given)) = old()? {
        for (id, value) in &given {
            match checked(section, &defaults, id, value, None) {
                Ok((id, value)) => {
                    moved.insert(id.to_owned(), value);
                }
                // What its field refuses now it would refuse each time: it stays behind.
                Err(refused) if refused.code == "invalid_input" => {}
                Err(error) => return Err(error),
            }
        }
    }
    let (revision, values) = if moved == defaults {
        (0, json!({}))
    } else {
        (1, Value::Object(moved))
    };
    let people = db.dsp(dsp)?;
    store::write(&people, section, None, revision, &values, None)
}

/// `now` with each of the `given` fields' values, each `checked`. It reads every list the
/// section offers first, so a section Settings leaves out, as one of them fails, isn't saved
/// either.
fn merged(
    section: &Section,
    now: &Map<String, Value>,
    given: &Map<String, Value>,
    mut lists: Option<&mut kinds::Lists>,
) -> Result<Map<String, Value>> {
    let mut after = now.clone();
    if let Some(lists) = lists.as_deref_mut() {
        for field in section.fields {
            lists.options(field)?;
        }
    }
    for (id, value) in given {
        let (id, value) = checked(section, now, id, value, lists.as_deref_mut())?;
        after.insert(id.to_owned(), value);
    }
    Ok(after)
}

/// The section's field `id` and `value` as it keeps it, checked as its kind says, against what
/// it holds `now` and the options it offers from `lists`: refused as `invalid_input` for a
/// field the section hasn't. With no lists, as for
/// values a feature carries over, a field whose options are listed as it's read takes any
/// value that fits it.
fn checked(
    section: &Section,
    now: &Map<String, Value>,
    id: &str,
    value: &Value,
    lists: Option<&mut kinds::Lists>,
) -> Result<(&'static str, Value)> {
    let field = section
        .fields
        .iter()
        .find(|field| field.id == id)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let options = match lists {
        Some(lists) => lists.options(field)?,
        None => None,
    };
    let value = kinds::checked(field, value, &now[field.id], options.as_deref())?;
    Ok((field.id, value))
}
