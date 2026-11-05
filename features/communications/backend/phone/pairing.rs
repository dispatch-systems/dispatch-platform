//! Pairing a phone: a code someone at the DSP shows, which the phone's app reads and trades for
//! a key of its own. The code works once, for ten minutes; only its SHA-256 is kept, as only the
//! key's is. A code tried is told right or wrong by a read (`opens`), so guessing never takes the
//! write lock, and each wrong one counts, in memory, against the address it came from and the DSP
//! it was tried at, each of which waits once it guessed too often.
use super::storage::DeviceStore;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use dispatch_core::{
    Error, Result, State,
    accounts::api::types::Dsp,
    db::{Store, now},
    ensure,
    foundation::crypto,
};
use qrcode::{QrCode, render::svg};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

/// The letters a code is made of: no 0 or O, no 1 or I, so one read aloud or typed is right.
const LETTERS: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// How many letters a code has.
pub const LENGTH: usize = 8;
/// How many random bytes a phone's key is: written in hexadecimal, twice as many letters.
pub const KEY_BYTES: usize = 32;
/// How long a wrong code counts against where it came from.
const GUESSES_MS: i64 = 10 * 60 * 1000;
/// The wrong codes an address may try within `GUESSES_MS` before it waits, as many as someone
/// mistyping one would; and a DSP be tried with, from every address, as a guesser changing
/// addresses would. Either is a few of the 32^8 codes there are.
pub const ADDRESS_GUESSES: usize = 10;
pub const DSP_GUESSES: usize = 100;
/// Addresses remembered at once; past it, those quiet for `GUESSES_MS` are forgotten first, and a
/// new one isn't counted while none is.
const MOST_ADDRESSES: usize = 2_000;

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
/// A new key for a phone: `KEY_BYTES` random bytes, in hexadecimal.
pub fn new_key() -> Result<String> {
    Ok(crypto::hex(&crypto::random::<KEY_BYTES>()?))
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

/// The DSP `at` reads, where the code kept as `code` (`kept`) pairs a phone now: told by a read,
/// a wrong code refused `phones_pairing_invalid` and counted against the address `ip` and the DSP.
/// Refused `rate_limited` while either guessed `ADDRESS_GUESSES` or `DSP_GUESSES` wrong in the last
/// `GUESSES_MS`: the address before the database is asked, and both again as the read answers,
/// counting under the same lock, as guesses from the address may have answered meanwhile.
pub async fn opens(
    state: &Arc<State>,
    ip: &str,
    at: impl FnOnce(&Store) -> Result<Dsp> + Send + 'static,
    code: String,
) -> Result<Dsp> {
    let asked = now();
    guessed().waits(Tried::Address(ip), asked)?;
    let (dsp, open) = state
        .read(move |db| {
            let dsp = at(db)?;
            let open = db.phones_pairing_open(&dsp.id, &code, asked)?;
            Ok((dsp, open))
        })
        .await?;
    let (mut counted, answered) = (guessed(), now());
    counted.waits(Tried::Address(ip), answered)?;
    counted.waits(Tried::Dsp(&dsp.id), answered)?;
    if !open {
        counted.wrong(ip, &dsp.id, answered);
        return Err(Error::new("phones_pairing_invalid", 401));
    }
    Ok(dsp)
}

/// Where wrong codes came from: an address, or the DSP they were tried at.
enum Tried<'a> {
    Address(&'a str),
    Dsp(&'a str),
}
/// When each address tried a wrong code lately, and each DSP was tried with one, the latest last.
#[derive(Default)]
struct Guessed {
    addresses: HashMap<String, VecDeque<i64>>,
    dsps: HashMap<String, VecDeque<i64>>,
}
impl Guessed {
    /// Refuses `rate_limited` while `tried` guessed as many wrong as it may by `at`.
    fn waits(&mut self, tried: Tried, at: i64) -> Result<()> {
        let (guesses, key, most) = match tried {
            Tried::Address(ip) => (&mut self.addresses, ip, ADDRESS_GUESSES),
            Tried::Dsp(dsp) => (&mut self.dsps, dsp, DSP_GUESSES),
        };
        if let Some(wrong) = guesses.get_mut(key) {
            recent(wrong, at);
            ensure(wrong.len() < most, "rate_limited", 429)?;
        }
        Ok(())
    }
    /// Counts a wrong code the address `ip` tried at the DSP `dsp`, at `at`.
    fn wrong(&mut self, ip: &str, dsp: &str, at: i64) {
        let crowded = self.addresses.len() >= MOST_ADDRESSES && !self.addresses.contains_key(ip);
        if crowded {
            self.addresses
                .retain(|_, wrong| wrong.back().is_some_and(|latest| *latest > at - GUESSES_MS));
        }
        if !crowded || self.addresses.len() < MOST_ADDRESSES {
            count(self.addresses.entry(ip.to_owned()).or_default(), at);
        }
        count(self.dsps.entry(dsp.to_owned()).or_default(), at);
    }
}
fn guessed() -> MutexGuard<'static, Guessed> {
    static GUESSED: LazyLock<Mutex<Guessed>> = LazyLock::new(Default::default);
    GUESSED.lock().unwrap_or_else(|poison| poison.into_inner())
}
/// `wrong`, of those within `GUESSES_MS` of `at`, with one more at `at`.
fn count(wrong: &mut VecDeque<i64>, at: i64) {
    recent(wrong, at);
    wrong.push_back(at);
}
/// Lets go of the wrong codes in `wrong` older than `GUESSES_MS` before `at`.
fn recent(wrong: &mut VecDeque<i64>, at: i64) {
    while wrong.front().is_some_and(|tried| *tried <= at - GUESSES_MS) {
        wrong.pop_front();
    }
}

#[cfg(test)]
#[path = "../../tests/backend/phone/pairing.rs"]
mod tests;
