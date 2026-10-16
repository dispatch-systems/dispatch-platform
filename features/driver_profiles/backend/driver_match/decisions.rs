//! The decisions a person makes about who is whom: two people are one, an ID belongs to
//! someone else, two people are different, or a match was right. Each is audited by codes.
use super::assign::new_person;
use super::legacy::mirror_links;
use super::{DriverMatchStore, valid_code};
use crate::api::types::DriverMatch;
use dispatch_core::{
    Result,
    accounts::Context,
    db::{Db, Store, iso},
    ensure,
    manifest::people::DriverSource,
};
use rusqlite::params;

/// Makes two people one: `code`'s IDs move to `into`, and `code` leads to `into` from
/// now on. A decision that the two were different people no longer stands.
pub(super) fn merge(store: &Store, c: &Context, code: &str, into: &str) -> Result<DriverMatch> {
    ensure(
        valid_code(code) && valid_code(into) && code != into,
        "invalid_input",
        400,
    )?;
    let dsp = c.dsp.id.as_str();
    let db = store.dsp(dsp)?;
    db.transaction(|| {
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
        db.exec("UPDATE people SET merged_into=?1 WHERE merged_into=?2", params![into, code])?;
        let others = db.query_as::<(String, String, String, Option<String>)>(
            "SELECT first,second,decided_at,actor_id FROM people_apart WHERE first=?1 OR second=?1",
            [code],
        )?;
        db.exec("DELETE FROM people_apart WHERE first=?1 OR second=?1", [code])?;
        for (first, second, at, by) in others {
            let other = if first == code { second } else { first };
            if other == into {
                continue;
            }
            let (a, b) = if into < other.as_str() { (into, other.as_str()) } else { (other.as_str(), into) };
            db.exec(
                "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
                params![a, b, at, by],
            )?;
        }
        mirror_links(&db)
    })?;
    drop(db);
    // Codes, not names: what a collection says about someone stays in its database.
    store.audit_ref(
        Some(c.actor()),
        Some(dsp),
        "driver_match.merged",
        &format!("{code} into {into}"),
        Some(&format!("{code} and {into}")),
        &[("code", Some(code.to_owned()), Some(into.to_owned()))],
        Some(("driver", into)),
    )?;
    store.driver_match(dsp)
}

/// Moves one ID off to a new person, when a link was wrong. The two are then kept
/// apart, so they are not suggested as one again.
pub(super) fn split(
    store: &Store,
    c: &Context,
    code: &str,
    source: DriverSource,
    id: &str,
) -> Result<DriverMatch> {
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
            "driver_changed",
            409,
        )?;
        ensure(
            db.count("SELECT count(*) FROM person_ids WHERE code=?", [code])? > 1,
            "driver_single_id",
            409,
        )?;
        let now = iso();
        let fresh = new_person(store, &db, dsp, &now, Some(code), Some(c.actor()))?;
        db.exec(
            "UPDATE person_ids SET code=?1,linked_by='person',linked_at=?2,actor_id=?3 \
             WHERE source=?4 AND external_id=?5",
            params![fresh, now, c.actor(), source, id],
        )?;
        let (a, b) = if code < fresh.as_str() {
            (code, fresh.as_str())
        } else {
            (fresh.as_str(), code)
        };
        db.exec(
            "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
            params![a, b, now, c.actor()],
        )?;
        mirror_links(&db)?;
        Ok(fresh)
    })?;
    drop(db);
    store.audit_ref(
        Some(c.actor()),
        Some(dsp),
        "driver_match.split",
        &format!("{code} to {fresh}"),
        Some(&fresh),
        &[("code", Some(code.to_owned()), Some(fresh.clone()))],
        Some(("driver", &fresh)),
    )?;
    store.driver_match(dsp)
}

/// Records that two people are different, so they are never suggested as one again.
pub(super) fn keep_apart(
    store: &Store,
    c: &Context,
    code: &str,
    other: &str,
) -> Result<DriverMatch> {
    ensure(
        valid_code(code) && valid_code(other) && code != other,
        "invalid_input",
        400,
    )?;
    let dsp = c.dsp.id.as_str();
    let db = store.dsp(dsp)?;
    let (first, second) = if code < other {
        (code, other)
    } else {
        (other, code)
    };
    db.transaction(|| {
        current(&db, code)?;
        current(&db, other)?;
        db.exec(
            "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
            params![first, second, iso(), c.actor()],
        )
    })?;
    drop(db);
    store.audit_ref(
        Some(c.actor()),
        Some(dsp),
        "driver_match.kept_apart",
        &format!("{code} and {other}"),
        Some(&format!("{code} and {other}")),
        &[],
        Some(("driver", code)),
    )?;
    store.driver_match(dsp)
}

pub(super) fn confirm(
    store: &Store,
    c: &Context,
    code: &str,
    source: DriverSource,
    id: &str,
) -> Result<DriverMatch> {
    ensure(valid_code(code), "invalid_input", 400)?;
    let dsp = c.dsp.id.as_str();
    let db = store.dsp(dsp)?;
    db.transaction(|| {
        current(&db, code)?;
        ensure(
            db.exec(
                "UPDATE person_ids SET linked_by='person',linked_at=?1,actor_id=?2 \
                 WHERE code=?3 AND source=?4 AND external_id=?5 AND linked_by<>'new'",
                params![iso(), c.actor(), code, source, id],
            )? == 1,
            "driver_changed",
            409,
        )?;
        mirror_links(&db)
    })?;
    drop(db);
    store.audit_ref(
        Some(c.actor()),
        Some(dsp),
        "driver_match.confirmed",
        code,
        Some(code),
        &[],
        Some(("driver", code)),
    )?;
    store.driver_match(dsp)
}

/// Answers whether `code` is a person who still holds their IDs.
pub(super) fn current(db: &Db, code: &str) -> Result<()> {
    ensure(
        db.count(
            "SELECT count(*) FROM people WHERE code=? AND merged_into IS NULL",
            [code],
        )? == 1,
        "driver_changed",
        409,
    )
}
