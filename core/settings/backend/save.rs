//! Saving a section: what's sent checked and merged onto what's kept, at the revision it was
//! read at, confirmed when its warning has words, and recorded in the DSP's log; and the
//! values a feature kept itself before, carried into its section once.
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
    Ok((
        Saved::Saved {
            revision,
            values: after,
        },
        lists,
    ))
}

/// Moves values a feature kept itself into its declared `section`, as the DSP's first, from
/// its `upgrade_storage` as each DSP opens: `old` reads them as the feature kept them, none
/// when it kept none. They're kept only while nothing of the section was ever saved, checked
/// as a save checks them but against no list a field takes as it's read, which may have
/// dropped what was chosen then; at revision 1, and not recorded in the log, whose entries
/// from before already say how they came to be. Values its fields refuse now leave the
/// defaults, as do none or the defaults themselves.
///
/// It never stops a DSP opening: what else fails, reading the old values or keeping them, is
/// noted in the server's log as `settings.carry_over_failed`, with the DSP, the section and
/// the error's code but never a value, and tried again as the DSP next opens, as nothing was
/// kept. Once the section holds values, moved or saved, it's one keyed read of its record and
/// `old` isn't called. Only `dsp`'s own database is read and written.
pub fn carry_over(
    db: &Store,
    dsp: &str,
    section: &Section,
    old: impl FnOnce() -> Result<Option<Value>>,
) {
    if let Err(error) = adopt(db, dsp, section, old)
        && error.code != "invalid_input"
    {
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
    let now = store::current(db, dsp, section, &store::record(db, dsp, section, None)?)?;
    if now.revision != 0 {
        return Ok(());
    }
    let Some(values) = old()? else {
        return Ok(());
    };
    let given = values
        .as_object()
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let after = merged(section, &now.values, given, None)?;
    if after != now.values {
        let people = db.dsp(dsp)?;
        store::write(&people, section, None, 1, &Value::Object(after), None)?;
    }
    Ok(())
}

/// `now` with each of the `given` fields' values, each checked as its kind says and against
/// the options its field offers from `lists`: refused as `invalid_input` for a field the
/// section hasn't, or a schedules field, which holds no value. It reads every list the section
/// offers first, so a section Settings leaves out, as one of them fails, isn't saved either.
/// With no lists, as for values a feature adopts, a field whose options are listed as it's
/// read takes any value that fits it.
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
        let field = section
            .fields
            .iter()
            .find(|field| field.id == id && kinds::default(field).is_some())
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let options = match lists.as_deref_mut() {
            Some(lists) => lists.options(field)?,
            None => None,
        };
        let checked = kinds::checked(field, value, &now[field.id], options.as_deref())?;
        after.insert(field.id.to_owned(), checked);
    }
    Ok(after)
}
