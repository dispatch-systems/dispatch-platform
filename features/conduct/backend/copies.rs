//! Photos of writeups' signed paper, kept in the DSP's own database with the rest of its data.
//! The page shrinks a photo before sending it, so each is small.
use super::writeups;
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Store, iso},
    ensure,
};
use rusqlite::params;

/// The most bytes one photo may be.
pub(crate) const LIMIT: u64 = 8 * 1024 * 1024;

/// Keeps `bytes` as the photo of the writeup `id`'s signed paper, in place of one kept before,
/// while the writeup is unsigned and still as read at `revision`: the paper photographed is the
/// notice as it reads, which signing it next at the same revision holds. A signed writeup's
/// photo stays, and a change to the writeup drops it.
pub(crate) fn keep(
    store: &Store,
    c: &Context,
    id: &str,
    revision: i64,
    bytes: &[u8],
) -> Result<()> {
    let media_type = picture_type(bytes)?;
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        writeups::unsigned(&db, id, Some(revision))?;
        db.exec(
            "INSERT INTO conduct_copies (writeup_id,media_type,bytes,added_by_name,added_at) \
             VALUES (?,?,?,?,?) ON CONFLICT(writeup_id) DO UPDATE SET media_type=excluded.media_type,\
             bytes=excluded.bytes,added_by_name=excluded.added_by_name,added_at=excluded.added_at",
            params![id, media_type, bytes, c.auth.user.name(), iso()],
        )?;
        Ok(())
    })
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
