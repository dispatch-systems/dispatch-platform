//! The decisions a person makes about who is whom: two people are one, an ID belongs to
//! someone else, two people are different, a match was right, or what someone does, whatever
//! their sources say. Each is audited by codes, never names: what a collection says about
//! someone stays in its database.
use super::audit::both;
use super::codes::{new_person, valid_code};
use crate::api::types::{GivenRole, IdLink};
use dispatch_core::{
    Result,
    accounts::Context,
    db::{AuditChange, Db, Store, iso},
    ensure,
    manifest::people::Source,
};
use rusqlite::params;

/// Makes two people one: `code`'s IDs move to `into`, and `code` leads to `into` from now
/// on. Whom `code` was kept apart from, `into` is kept apart from, and the role someone gave
/// `code` goes to `into` unless they have one, audited with the merge.
pub(super) fn merge(store: &Store, c: &Context, code: &str, into: &str) -> Result<()> {
    ensure(
        valid_code(code) && valid_code(into) && code != into,
        "invalid_input",
        400,
    )?;
    let db = store.dsp(&c.dsp.id)?;
    // The role `into` took from `code`.
    let taken = db.transaction(|| {
        current(&db, code)?;
        current(&db, into)?;
        let now = iso();
        db.exec(
            "UPDATE person_ids SET code=?1,linked_by='person',linked_at=?2,actor_id=?3 \
             WHERE code=?4",
            params![into, now, c.actor(), code],
        )?;
        db.exec(
            "UPDATE people SET merged_into=?1,merged_at=?2,merged_by=?3 WHERE code=?4",
            params![into, now, c.actor(), code],
        )?;
        // Codes merged into this one before lead straight to the survivor now.
        db.exec(
            "UPDATE people SET merged_into=?1 WHERE merged_into=?2",
            params![into, code],
        )?;
        let others = db.query_as::<(String, String, String, Option<String>)>(
            "SELECT first,second,decided_at,actor_id FROM people_apart WHERE first=?1 OR second=?1",
            [code],
        )?;
        db.exec(
            "DELETE FROM people_apart WHERE first=?1 OR second=?1",
            [code],
        )?;
        for (first, second, at, by) in others {
            let other = if first == code { second } else { first };
            if other != into {
                apart(&db, into, &other, &at, by.as_deref())?;
            }
        }
        let given = given_role(&db, code)?;
        let moved = db.exec(
            "INSERT OR IGNORE INTO person_roles(code,role,decided_at,actor_id) \
             SELECT ?1,role,decided_at,actor_id FROM person_roles WHERE code=?2",
            params![into, code],
        )? == 1;
        db.exec("DELETE FROM person_roles WHERE code=?", [code])?;
        Ok(given.filter(|_| moved))
    })?;
    drop(db);
    let mut changes = vec![("code", Some(code.to_owned()), Some(into.to_owned()))];
    if let Some(given) = taken {
        changes.push(("role", None, Some(given.label().to_owned())));
    }
    audited(
        store,
        c,
        "driver_match.merged",
        &format!("{code} into {into}"),
        &both(code, into),
        &changes,
        into,
    )
}

/// Moves one ID off to a new person, when a link was wrong. The two are then kept apart, so
/// they are not suggested as one again.
pub(super) fn split(
    store: &Store,
    c: &Context,
    code: &str,
    source: Source,
    id: &str,
) -> Result<()> {
    ensure(valid_code(code), "invalid_input", 400)?;
    let dsp = c.dsp.id.as_str();
    let db = store.dsp(dsp)?;
    let fresh = db.transaction(|| {
        current(&db, code)?;
        ensure(
            db.count(
                "SELECT count(*) FROM person_ids WHERE source=? AND external_id=? AND code=?",
                params![source, id, code],
            )? == 1,
            "person_changed",
            409,
        )?;
        ensure(
            db.count("SELECT count(*) FROM person_ids WHERE code=?", [code])? > 1,
            "person_single_id",
            409,
        )?;
        let now = iso();
        let fresh = new_person(store, &db, dsp, &now, Some(code), Some(c.actor()))?;
        db.exec(
            "UPDATE person_ids SET code=?1,linked_by='person',linked_at=?2,actor_id=?3 \
             WHERE source=?4 AND external_id=?5",
            params![fresh, now, c.actor(), source, id],
        )?;
        apart(&db, code, &fresh, &now, Some(c.actor()))?;
        Ok(fresh)
    })?;
    drop(db);
    audited(
        store,
        c,
        "driver_match.split",
        &format!("{code} to {fresh}"),
        &fresh,
        &[("code", Some(code.to_owned()), Some(fresh.clone()))],
        &fresh,
    )
}

/// Records that two people are different, so they are never suggested as one again.
pub(super) fn keep_apart(store: &Store, c: &Context, code: &str, other: &str) -> Result<()> {
    ensure(
        valid_code(code) && valid_code(other) && code != other,
        "invalid_input",
        400,
    )?;
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        current(&db, code)?;
        current(&db, other)?;
        apart(&db, code, other, &iso(), Some(c.actor()))
    })?;
    drop(db);
    let pair = both(code, other);
    audited(store, c, "driver_match.kept_apart", &pair, &pair, &[], code)
}

/// Confirms one of `code`'s IDs is theirs, when a match by a name or a contact joined it.
pub(super) fn confirm(
    store: &Store,
    c: &Context,
    code: &str,
    source: Source,
    id: &str,
) -> Result<()> {
    ensure(valid_code(code), "invalid_input", 400)?;
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        current(&db, code)?;
        let link: Option<(IdLink,)> = db.one_as(
            "SELECT linked_by FROM person_ids WHERE code=? AND source=? AND external_id=?",
            params![code, source, id],
        )?;
        ensure(
            link.is_some_and(|(link,)| link.confirmable()),
            "person_changed",
            409,
        )?;
        db.exec(
            "UPDATE person_ids SET linked_by='person',linked_at=?1,actor_id=?2 \
             WHERE code=?3 AND source=?4 AND external_id=?5",
            params![iso(), c.actor(), code, source, id],
        )?;
        Ok(())
    })?;
    drop(db);
    audited(store, c, "driver_match.confirmed", code, code, &[], code)
}

/// Gives `code` a role, or with `None` leaves theirs to their sources again. Giving the role
/// they have changes nothing.
pub(super) fn give_role(
    store: &Store,
    c: &Context,
    code: &str,
    given: Option<GivenRole>,
) -> Result<()> {
    ensure(valid_code(code), "invalid_input", 400)?;
    let db = store.dsp(&c.dsp.id)?;
    // The role they had, when this one changes it.
    let changed = db.transaction(|| {
        current(&db, code)?;
        let before = given_role(&db, code)?;
        if before == given {
            return Ok(None);
        }
        match given {
            Some(given) => db.exec(
                "INSERT OR REPLACE INTO person_roles(code,role,decided_at,actor_id) \
                 VALUES (?,?,?,?)",
                params![code, given, iso(), c.actor()],
            ),
            None => db.exec("DELETE FROM person_roles WHERE code=?", [code]),
        }?;
        Ok(Some(before))
    })?;
    drop(db);
    let Some(before) = changed else {
        return Ok(());
    };
    let label = |given: Option<GivenRole>| given.map(|given| given.label().to_owned());
    audited(
        store,
        c,
        "driver_match.role_given",
        code,
        code,
        &[("role", label(before), label(given))],
        code,
    )
}

/// The role someone gave `code`.
fn given_role(db: &Db, code: &str) -> Result<Option<GivenRole>> {
    Ok(db
        .one_as::<(GivenRole,)>("SELECT role FROM person_roles WHERE code=?", [code])?
        .map(|(given,)| given))
}

/// Answers whether `code` is a person who still holds their IDs; a decision about anyone
/// else was made on what has since changed.
fn current(db: &Db, code: &str) -> Result<()> {
    ensure(
        db.count(
            "SELECT count(*) FROM people WHERE code=? AND merged_into IS NULL",
            [code],
        )? == 1,
        "person_changed",
        409,
    )
}

/// Keeps two people apart, as decided `at` by `actor`, the pair stored in the order of their
/// codes.
fn apart(db: &Db, a: &str, b: &str, at: &str, actor: Option<&str>) -> Result<()> {
    let (first, second) = if a < b { (a, b) } else { (b, a) };
    db.exec(
        "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
        params![first, second, at, actor],
    )?;
    Ok(())
}

/// Audits a decision about the person `code`: `detail` says what happened by codes, and
/// `target` names the codes the activity log puts today's names to.
fn audited(
    store: &Store,
    c: &Context,
    action: &str,
    detail: &str,
    target: &str,
    changes: &[AuditChange],
    code: &str,
) -> Result<()> {
    store.audit_ref(
        Some(c.actor()),
        Some(&c.dsp.id),
        action,
        detail,
        Some(target),
        changes,
        Some(("driver", code)),
    )?;
    Ok(())
}
