//! Pairing a phone: a code someone at the DSP shows, which the phone's app reads and trades for
//! a key of its own. The code works once, for ten minutes; only its SHA-256 is kept, as only the
//! key's is.
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use dispatch_core::{Error, Result, foundation::crypto};
use qrcode::{QrCode, render::svg};

/// The letters a code is made of: no 0 or O, no 1 or I, so one read aloud or typed is right.
const LETTERS: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// How many letters a code has.
const LENGTH: usize = 8;

/// A new code: eight of `LETTERS`.
pub fn new_code() -> Result<String> {
    Ok(crypto::random::<LENGTH>()?
        .iter()
        .map(|byte| LETTERS[usize::from(byte & 31)] as char)
        .collect())
}
/// How a code is kept: the SHA-256 of its letters, in capitals, without spaces or dashes.
pub fn kept(code: &str) -> Option<String> {
    let letters: String = code
        .chars()
        .filter(|c| !matches!(c, ' ' | '-'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (letters.len() == LENGTH && letters.bytes().all(|b| LETTERS.contains(&b)))
        .then(|| crypto::sha(letters))
}
/// A new key for a phone: 32 random bytes, in hexadecimal.
pub fn new_key() -> Result<String> {
    Ok(crypto::hex(&crypto::random::<32>()?))
}
/// What the phone's app reads from the code's picture: the DSP's address, and the code.
pub fn link(site: &str, code: &str) -> String {
    format!("dispatch-phone:pair?site={site}&code={code}")
}
/// `link` as a picture of a QR code, a `data:` address an `<img>` shows.
pub fn picture(link: &str) -> Result<String> {
    let svg = QrCode::new(link.as_bytes())
        .map_err(|_| Error::new("phones_pairing_unavailable", 503))?
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .build();
    Ok(format!("data:image/svg+xml;base64,{}", BASE64.encode(svg)))
}

#[cfg(test)]
#[path = "../tests/backend/pairing.rs"]
mod tests;
