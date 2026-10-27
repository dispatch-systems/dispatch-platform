//! What someone at the DSP writes, checked before it goes, against what one message of its kind
//! may carry (`Limits`): a Slack message's (`SLACK`), or a text's as whatever offers the DSP
//! texting says (`texting.limits()`). Either carries photos of the same kinds (`PHOTO_KINDS`).
use super::model::Reference;
use crate::api::types::ComposeLimits;
use dispatch_core::{Result, db::Store, ensure};
use dispatch_phones_contract::{self as texting, Texting};

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
impl Limits {
    /// The most characters of a message's text read before it's trimmed and checked: twice what
    /// it may carry, for the spaces around it.
    pub fn read(self) -> usize {
        self.text * 2
    }
}
impl From<Limits> for ComposeLimits {
    fn from(limits: Limits) -> Self {
        Self {
            text: limits.text,
            photos: limits.photos,
            largest: limits.largest,
            kinds: PHOTO_KINDS
                .iter()
                .map(|(kind, _)| (*kind).to_owned())
                .collect(),
        }
    }
}

/// What a text may carry, as `texting` says.
pub fn texted(texting: &dyn Texting) -> Limits {
    let limits = texting.limits();
    Limits {
        text: limits.body,
        photos: limits.files,
        largest: limits.largest,
    }
}
/// What a message in the conversation `reference` may carry: a Slack one's, or by text as what
/// offers the DSP texting says, none while nothing does.
pub fn of(db: &Store, dsp: &str, reference: &Reference) -> Result<Option<Limits>> {
    Ok(match reference {
        Reference::Slack(_) => Some(SLACK),
        Reference::Text(_) => texting::for_dsp(db, dsp)?.map(texted),
    })
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
