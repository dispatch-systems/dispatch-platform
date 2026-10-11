use super::{api::types::SupportAttachment, authorize, found};
use crate::{
    Result,
    accounts::Auth,
    db::{FromRow, Row, Store, now},
    ensure,
    foundation::crypto,
};
use rusqlite::{OptionalExtension, params};

pub const LIMIT: u64 = 20 * 1024 * 1024;
impl FromRow for SupportAttachment {
    fn from_row(r: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            name: r.get("name")?,
            media_type: r.get("media_type")?,
            size: r.get("size")?,
            private: r.get("private")?,
        })
    }
}
pub(crate) fn list(db: &Store, ticket: &str, private: bool) -> Result<Vec<SupportAttachment>> {
    db.platform.query_as(
        "SELECT id,name,media_type,length(bytes) size,private FROM support_attachments \
         WHERE ticket_id=? AND (private=0 OR ?) ORDER BY created_at,id",
        params![ticket, private],
    )
}
fn media(bytes: &[u8]) -> Result<&'static str> {
    let kind = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else {
        None
    };
    kind.ok_or_else(|| crate::Error::new("support_image_required", 400))
}
pub fn save(db: &Store, auth: &Auth, name: &str, bytes: &[u8]) -> Result<SupportAttachment> {
    store(
        db,
        &auth.user.id,
        auth.scope.as_deref().unwrap_or(""),
        None,
        false,
        name,
        bytes,
    )
}
pub(crate) fn store(
    db: &Store,
    user: &str,
    scope: &str,
    ticket: Option<&str>,
    private: bool,
    name: &str,
    bytes: &[u8],
) -> Result<SupportAttachment> {
    ensure(
        !bytes.is_empty() && bytes.len() as u64 <= LIMIT,
        "upload_too_large",
        413,
    )?;
    ensure(
        !name.trim().is_empty() && name.chars().count() <= 180,
        "invalid_input",
        400,
    )?;
    let media_type = media(bytes)?.to_owned();
    let attachment = SupportAttachment {
        id: crypto::id("support_image")?,
        name: name.to_owned(),
        media_type,
        size: bytes.len() as u32,
        private,
    };
    db.platform.transaction(|| {
        db.platform.exec(
            "DELETE FROM support_attachments WHERE ticket_id IS NULL AND created_at<?",
            [now() - 86400000],
        )?;
        db.platform.exec(
            "INSERT INTO support_attachments(id,ticket_id,uploader_id,uploader_scope,name,media_type,bytes,private,created_at) \
             VALUES(?,?,?,?,?,?,?,?,?)",
            params![attachment.id, ticket, user, scope, name, attachment.media_type, bytes, private, now()],
        )?;
        Ok(attachment)
    })
}
pub(crate) fn assign(db: &Store, auth: &Auth, ticket: &str, ids: &[String]) -> Result<()> {
    for id in ids {
        let changed = db.platform.exec(
            "UPDATE support_attachments SET ticket_id=? WHERE id=? AND ticket_id IS NULL AND uploader_id=? AND uploader_scope=?",
            params![ticket, id, auth.user.id, auth.scope.as_deref().unwrap_or("")],
        )?;
        ensure(changed == 1, "support_attachment_unavailable", 409)?;
    }
    Ok(())
}
pub fn read(db: &Store, auth: &Auth, id: &str) -> Result<(String, Vec<u8>)> {
    let meta = found(db.platform.one(
        "SELECT ticket_id,uploader_id,uploader_scope,private FROM support_attachments WHERE id=?",
        [id],
    )?)?;
    if let Some(ticket) = meta["ticket_id"].as_str() {
        authorize(db, auth, ticket)?;
        ensure(
            auth.user.platform_owner || meta["private"] == 0,
            "support_ticket_not_found",
            404,
        )?;
    } else {
        ensure(
            meta["uploader_id"] == auth.user.id
                && meta["uploader_scope"] == auth.scope.as_deref().unwrap_or(""),
            "support_ticket_not_found",
            404,
        )?;
    }
    bytes(db, id)
}
pub(crate) fn bytes(db: &Store, id: &str) -> Result<(String, Vec<u8>)> {
    found(
        db.platform
            .0
            .query_row(
                "SELECT media_type,bytes FROM support_attachments WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
    )
}
