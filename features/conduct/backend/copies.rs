//! Photos of writeups' signed paper, kept in the DSP's own database with the rest of its data.
//! The page shrinks a photo before sending it, so each is small.
use super::{backups, writeups};
use crate::api::types::ConductWriteup;
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, Store, iso},
    ensure,
};
use rusqlite::params;

/// The most bytes one photo may be.
pub(crate) const LIMIT: u64 = 8 * 1024 * 1024;

/// Keeps `bytes` as the photo of the writeup `id`'s signed paper, in place of one kept before,
/// while the writeup is still as read at `revision`. Until it's signed, anyone who uses Conduct
/// keeps one, of the paper the notice prints as now, and a change to the notice drops it. Once
/// it's signed, only a member who manages Conduct keeps one, noted in the activity log, and the
/// copy backed up of the photo before is taken back once the new one is saved. Keeping one
/// moves the writeup to its next revision, which signing it names, so a sign that read it
/// before another photo was kept is refused.
pub(crate) fn keep(
    store: &Store,
    c: &Context,
    id: &str,
    revision: i64,
    bytes: &[u8],
) -> Result<()> {
    keep_requested(store, c, id, revision, bytes, None)
}

pub(crate) fn keep_requested(
    store: &Store,
    c: &Context,
    id: &str,
    revision: i64,
    bytes: &[u8],
    request: Option<&str>,
) -> Result<()> {
    let media_type = picture_type(bytes)?;
    let digest = dispatch_core::foundation::crypto::sha(bytes);
    let operation = super::requests::Operation {
        request,
        kind: "copy",
        writeup: id,
        revision,
        payload: &digest,
    };
    let db = store.dsp(&c.dsp.id)?;
    super::logged(store, &db, || {
        if let Some(result) = operation.previous(&db, c)? {
            keepable(&db, c, id, result)?;
            return Ok(());
        }
        let writeup = keepable(&db, c, id, revision)?;
        db.exec(
            "INSERT INTO conduct_copies (writeup_id,media_type,bytes,added_by_name,added_at) \
             VALUES (?,?,?,?,?) ON CONFLICT(writeup_id) DO UPDATE SET media_type=excluded.media_type,\
             bytes=excluded.bytes,added_by_name=excluded.added_by_name,added_at=excluded.added_at",
            params![id, media_type, bytes, c.auth.user.name(), iso()],
        )?;
        db.exec(
            "UPDATE conduct_writeups SET revision=revision+1,updated_at=? WHERE id=?",
            params![iso(), id],
        )?;
        operation.record(&db, c, revision + 1)?;
        if writeup.row.signed_on.is_none() {
            return Ok(());
        }
        backups::replaced(&db, id)?;
        // By its id alone: the activity log keeps no one's disciplinary record.
        if writeup.row.has_copy {
            c.audit(store, "conduct.copy_replaced", id)
        } else {
            c.audit(store, "conduct.copy_added", id)
        }
    })
}

/// The writeup `id`, while it's still as read at `revision` and the member `c` may keep a photo
/// of its paper: anyone who uses Conduct until it's signed, then only who manages it.
fn keepable(db: &Db, c: &Context, id: &str, revision: i64) -> Result<ConductWriteup> {
    let writeup = writeups::one(db, id)?;
    ensure(
        writeup.row.revision == revision,
        "conduct_writeup_changed",
        409,
    )?;
    ensure(
        writeup.row.signed_on.is_none() || c.can("conduct.manage"),
        "permission_denied",
        403,
    )?;
    Ok(writeup)
}

/// The photo of the writeup `id`'s signed paper: its media type and bytes.
pub(crate) fn read(store: &Store, dsp: &str, id: &str) -> Result<(String, Vec<u8>)> {
    store
        .dsp(dsp)?
        .one_as::<(String, Vec<u8>)>(
            "SELECT media_type,bytes FROM conduct_copies WHERE writeup_id=?",
            [id],
        )?
        .ok_or_else(|| Error::new("conduct_copy_not_found", 404))
}

/// The media type of a photo by its first bytes: a JPEG, PNG or WebP picture, nothing else.
fn picture_type(bytes: &[u8]) -> Result<&'static str> {
    ensure(
        !bytes.is_empty() && bytes.len() as u64 <= LIMIT,
        "upload_too_large",
        413,
    )?;
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Ok("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("image/png")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Ok("image/webp")
    } else {
        Err(Error::new("conduct_copy_not_picture", 400))
    }
}
