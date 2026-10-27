//! What someone at the DSP writes, checked before it goes, against what one message may carry
//! (`Limits`), as a Slack message's are (`SLACK`). Either kind carries photos of the same kinds
//! (`PHOTO_KINDS`).
use dispatch_core::{Result, ensure};

/// What a Slack message may carry: as much text as Slack shows in a section of one, and up to ten
/// photos of up to 20 MB each.
pub const SLACK: Limits = Limits {
    text: 3000,
    photos: 10,
    largest: 20 * 1024 * 1024,
};
/// The kinds of photo a message carries, in Slack and by text, each with how its bytes begin.
pub const PHOTO_KINDS: &[(&str, &[u8])] = &[
    ("image/png", b"\x89PNG"),
    ("image/jpeg", b"\xff\xd8\xff"),
    ("image/gif", b"GIF8"),
];

/// What one message may carry: the most characters its text may have, how many photos, and the
/// largest each may be, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub text: usize,
    pub photos: usize,
    pub largest: u64,
}

/// What someone at the DSP wrote, trimmed, with the photos they added: refused when it's
/// nothing, past `limits`, or with a photo twice.
pub fn checked<'a>(written: &'a str, photos: &[String], limits: Limits) -> Result<&'a str> {
    let written = written.trim();
    let distinct = photos
        .iter()
        .enumerate()
        .all(|(n, photo)| !photos[..n].contains(photo));
    ensure(
        (!written.is_empty() || !photos.is_empty())
            && written.chars().count() <= limits.text
            && photos.len() <= limits.photos
            && distinct,
        "invalid_input",
        400,
    )?;
    Ok(written)
}

/// How a photo of the kind `mime` begins, for a kind a message carries.
pub fn photo_kind(mime: &str) -> Option<&'static [u8]> {
    PHOTO_KINDS
        .iter()
        .find(|(kind, _)| *kind == mime)
        .map(|(_, begins)| *begins)
}
