//! The DSP's writeups: written, changed until signed, marked signed, and deleted. A signed
//! writeup never changes again, and only a member who manages Conduct deletes one.
use super::categories;
use crate::api::types::{ConductWriteup, ConductWriteupRow, WriteupInput};
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, Store, iso},
    ensure,
    foundation::crypto,
};
use rusqlite::params;

/// Who a writeup is for, as the directory knew them when it was written.
pub(crate) struct Subject {
    pub code: String,
    pub name: String,
}

/// What a writeup's row selects, with whether a photo of its signed paper is kept.
const SELECT: &str = "SELECT w.*, EXISTS(SELECT 1 FROM conduct_copies k WHERE k.writeup_id=w.id) has_copy \
     FROM conduct_writeups w";

pub(crate) const ROWS: &str = "SELECT w.id,w.person,w.person_name,w.category_id,w.tier,w.final_kind,\
     w.suspension,w.written_on,w.supervisor_name,w.signed_on,w.revision,w.created_at,\
     EXISTS(SELECT 1 FROM conduct_copies k WHERE k.writeup_id=w.id) has_copy FROM conduct_writeups w";

/// Every writeup, the newest first.
pub(crate) fn rows(db: &Db) -> Result<Vec<ConductWriteupRow>> {
    db.query_as(
        &format!("{ROWS} ORDER BY w.written_on DESC, w.created_at DESC, w.id"),
        [],
    )
}
/// The writeup `id`, whole.
pub(crate) fn one(db: &Db, id: &str) -> Result<ConductWriteup> {
    db.one_as(&format!("{SELECT} WHERE w.id=?"), [id])?
        .ok_or_else(|| Error::new("conduct_writeup_not_found", 404))
}

/// The DSP `dsp`'s writeup `id`, whole.
pub(crate) fn read(store: &Store, dsp: &str, id: &str) -> Result<ConductWriteup> {
    let db = store.dsp(dsp)?;
    one(&db, id)
}

/// Keeps a new writeup for `subject`, written by the member `c`, dated no later than `today`.
pub(crate) fn add(
    store: &Store,
    c: &Context,
    subject: &Subject,
    input: &WriteupInput,
    today: &str,
) -> Result<String> {
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        let category = categories::one(&db, &input.category)?;
        ensure(!category.retired, "conduct_category_not_found", 404)?;
        ensure(
            input.written_on.as_str() <= today,
            "conduct_date_ahead",
            400,
        )?;
        let id = crypto::id("writeup")?;
        let at = iso();
        db.exec(
            "INSERT INTO conduct_writeups (id,person,person_name,category_id,tier,final_kind,\
             suspension,written_on,supervisor_id,supervisor_name,statement,prior,plan,created_at,\
             updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                id,
                subject.code,
                subject.name,
                category.id,
                input.tier,
                input.final_kind,
                input.suspension,
                input.written_on,
                c.auth.user.id,
                c.auth.user.name(),
                input.statement,
                input.prior,
                input.plan,
                at,
                at
            ],
        )?;
        Ok(id)
    })
}

/// Changes the writeup `id` while it's unsigned, as it was read at `input.revision`.
pub(crate) fn change(
    store: &Store,
    c: &Context,
    id: &str,
    subject: &Subject,
    input: &WriteupInput,
    today: &str,
) -> Result<()> {
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        let before = unsigned(&db, id, input.revision)?;
        let category = categories::one(&db, &input.category)?;
        // A retired category stays only on the writeups already in it.
        ensure(
            !category.retired || category.id == before.row.category_id,
            "conduct_category_not_found",
            404,
        )?;
        ensure(
            input.written_on.as_str() <= today,
            "conduct_date_ahead",
            400,
        )?;
        db.exec(
            "UPDATE conduct_writeups SET person=?,person_name=?,category_id=?,tier=?,final_kind=?,\
             suspension=?,written_on=?,statement=?,prior=?,plan=?,revision=revision+1,updated_at=? \
             WHERE id=?",
            params![
                subject.code,
                subject.name,
                category.id,
                input.tier,
                input.final_kind,
                input.suspension,
                input.written_on,
                input.statement,
                input.prior,
                input.plan,
                iso(),
                id
            ],
        )?;
        // A photo was of the paper as it read before.
        db.exec("DELETE FROM conduct_copies WHERE writeup_id=?", [id])?;
        Ok(())
    })
}

/// Marks the writeup `id` signed on `signed_on`, a day from its own to `today`: it never
/// changes again.
pub(crate) fn sign(
    store: &Store,
    c: &Context,
    id: &str,
    revision: i64,
    signed_on: &str,
    today: &str,
) -> Result<()> {
    sign_requested(store, c, id, revision, signed_on, today, None)
}

pub(crate) fn sign_requested(
    store: &Store,
    c: &Context,
    id: &str,
    revision: i64,
    signed_on: &str,
    today: &str,
    request: Option<&str>,
) -> Result<()> {
    let operation = super::requests::Operation {
        request,
        kind: "sign",
        writeup: id,
        revision,
        payload: signed_on,
    };
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        if let Some(result) = operation.previous(&db, c)? {
            let kept = one(&db, id)?;
            ensure(
                kept.row.revision == result && kept.row.signed_on.as_deref() == Some(signed_on),
                "conduct_writeup_changed",
                409,
            )?;
            return Ok(());
        }
        let writeup = unsigned(&db, id, Some(revision))?;
        ensure(
            writeup.row.written_on.as_str() <= signed_on && signed_on <= today,
            "conduct_signed_date",
            400,
        )?;
        db.exec(
            "UPDATE conduct_writeups SET signed_on=?,signed_by_name=?,signed_at=?,\
             revision=revision+1 WHERE id=?",
            params![signed_on, c.auth.user.name(), iso(), id],
        )?;
        operation.record(&db, c, revision + 1)?;
        Ok(())
    })
}

/// Deletes the writeup `id` as read at `revision`, with its photo and what's kept of backing
/// that up, noting it in the DSP's activity log: a signed one only for a member who manages
/// Conduct.
pub(crate) fn delete(store: &Store, c: &Context, id: &str, revision: i64) -> Result<()> {
    let db = store.dsp(&c.dsp.id)?;
    super::logged(store, &db, || {
        let writeup = deletable(&db, c, id, revision)?;
        db.exec("DELETE FROM conduct_requests WHERE writeup_id=?", [id])?;
        db.exec("DELETE FROM conduct_backups WHERE writeup_id=?", [id])?;
        db.exec("DELETE FROM conduct_copies WHERE writeup_id=?", [id])?;
        db.exec("DELETE FROM conduct_writeups WHERE id=?", [id])?;
        // By its id alone: the activity log keeps no one's disciplinary record.
        c.audit(store, "conduct.writeup_deleted", &writeup.row.id)
    })
}

/// The writeup `id`, while it's still as read at `revision` and the member `c` may delete it:
/// anyone who uses Conduct until it's signed, then only who manages it.
pub(crate) fn deletable(db: &Db, c: &Context, id: &str, revision: i64) -> Result<ConductWriteup> {
    let writeup = one(db, id)?;
    ensure(
        writeup.row.signed_on.is_none() || c.can("conduct.manage"),
        "permission_denied",
        403,
    )?;
    ensure(
        writeup.row.revision == revision,
        "conduct_writeup_changed",
        409,
    )?;
    Ok(writeup)
}

/// The writeup `id`, while nobody has marked it signed and it's still as read at `revision`.
pub(crate) fn unsigned(db: &Db, id: &str, revision: Option<i64>) -> Result<ConductWriteup> {
    let writeup = one(db, id)?;
    ensure(
        writeup.row.signed_on.is_none(),
        "conduct_writeup_signed",
        409,
    )?;
    ensure(
        revision.is_none_or(|r| r == writeup.row.revision),
        "conduct_writeup_changed",
        409,
    )?;
    Ok(writeup)
}

#[cfg(test)]
#[path = "../tests/backend/writeups.rs"]
mod tests;
