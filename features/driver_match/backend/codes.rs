//! People's codes: six letters and digits that can't be mistaken for one another, unique
//! across the platform, so a code never leads to two people anywhere.
use dispatch_core::{
    Error, Result,
    db::{Db, Store},
    foundation::crypto,
};
use rusqlite::params;

/// Letters and digits that cannot be mistaken for one another, without U, so a code rarely
/// spells a word.
const ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const LENGTH: usize = 6;
/// The bytes below the largest multiple of the alphabet's length a byte holds: larger ones
/// would favour its first letters, so they are skipped.
const FAIR: u8 = (256 / ALPHABET.len() * ALPHABET.len()) as u8;
/// How many random bytes are drawn for a code: enough for six fair ones nearly always.
const DRAWN: usize = 24;
/// How many codes are tried before giving up, each taken already somewhere on the platform.
const TRIES: usize = 16;

/// Whether `code` is written as codes are.
pub(super) fn valid_code(code: &str) -> bool {
    code.len() == LENGTH && code.bytes().all(|b| ALPHABET.contains(&b))
}

/// Lets the codes of a DSP being deleted go: its people go with it.
pub(crate) fn forget_dsp(store: &Store, dsp: &str) -> Result<()> {
    store
        .platform
        .exec("DELETE FROM driver_codes WHERE dsp_id=?", [dsp])?;
    Ok(())
}

/// How a new person's code is drawn.
pub(super) enum Draw<'a> {
    /// At random, as every real DSP's people get theirs.
    Random,
    /// From the DSP's ID and `seed`, the first ID the person holds, as a demo's people get
    /// theirs: the same each time its demo data is made afresh under that DSP's ID, so a link to
    /// one of them still opens them.
    Demo(&'a str),
}
impl Draw<'_> {
    /// The bytes of the code tried `tried` times already.
    fn bytes(&self, dsp: &str, tried: usize) -> Result<[u8; DRAWN]> {
        match self {
            Self::Random => crypto::random(),
            Self::Demo(seed) => {
                let drawn = crypto::hmac(dsp.as_bytes(), &format!("{seed} {tried}"));
                Ok(std::array::from_fn(|i| drawn[i]))
            }
        }
    }
}

/// A person with a new code, listed in the platform's `driver_codes`.
pub(super) fn new_person(
    store: &Store,
    db: &Db,
    dsp: &str,
    now: &str,
    split_from: Option<&str>,
    actor: Option<&str>,
    draw: Draw<'_>,
) -> Result<String> {
    for tried in 0..TRIES {
        let code: String = draw
            .bytes(dsp, tried)?
            .into_iter()
            .filter(|b| *b < FAIR)
            .take(LENGTH)
            .map(|b| ALPHABET[usize::from(b) % ALPHABET.len()] as char)
            .collect();
        if code.len() == LENGTH
            && store.platform.exec(
                "INSERT OR IGNORE INTO driver_codes(code,dsp_id,created_at) VALUES (?,?,?)",
                params![code, dsp, now],
            )? == 1
        {
            db.exec(
                "INSERT INTO people(code,created_at,split_from,created_by) VALUES (?,?,?,?)",
                params![code, now, split_from, actor],
            )?;
            return Ok(code);
        }
    }
    Err(Error::new("person_code_unavailable", 500))
}
