//! Phone numbers: one number written one way, as `texting` keeps them (`normal`); how one reads
//! (`shown`); the ten digits Routes keeps a work phone by (`ten_digits`); and what the DSP keeps
//! of the numbers that text it, and what its phones are called, to read its texts by (`Numbers`).
use super::storage::TextStore;
use dispatch_core::{Result, db::Store};
use dispatch_phones_contract as texting;
use std::collections::{BTreeSet, HashMap};

/// What the DSP keeps of the numbers that text it, and what its phones are called, to read its
/// texts by.
pub struct Numbers {
    contacts: HashMap<String, String>,
    blocked: BTreeSet<String>,
    phones: HashMap<String, String>,
}
impl Numbers {
    pub fn read(db: &Store, dsp: &str) -> Result<Self> {
        let phones = match texting::for_dsp(db, dsp)? {
            Some(texting) => texting
                .phones(db, dsp)?
                .into_iter()
                .map(|phone| (phone.id, phone.name))
                .collect(),
            None => HashMap::new(),
        };
        Ok(Self {
            contacts: db.communications_contacts(dsp)?.into_iter().collect(),
            blocked: db.communications_blocked(dsp)?,
            phones,
        })
    }
    /// The name the DSP saved for `number`, if it did.
    pub fn contact(&self, number: &str) -> Option<&str> {
        self.contacts.get(number).map(String::as_str)
    }
    /// Every number the DSP saved a name for.
    pub fn contacts(&self) -> impl Iterator<Item = &str> {
        self.contacts.keys().map(String::as_str)
    }
    pub fn blocked(&self, number: &str) -> bool {
        self.blocked.contains(number)
    }
    /// Every number blocked.
    pub fn all_blocked(&self) -> impl Iterator<Item = &str> {
        self.blocked.iter().map(String::as_str)
    }
    /// What the DSP's phone `id` is called, while it's paired.
    pub fn phone(&self, id: &str) -> Option<&str> {
        self.phones.get(id).map(String::as_str)
    }
}

/// `raw` written as `texting` keeps numbers, so one number is always written one way: `+1` and ten
/// digits in North America, `+` and its digits with a country code, a short code's digits alone;
/// none for what isn't a number.
pub fn normal(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 32
        || !raw
            .chars()
            .all(|c| c.is_ascii_digit() || " +-().".contains(c))
    {
        return None;
    }
    let plus = raw.starts_with('+');
    let bare = raw.chars().all(|c| c.is_ascii_digit());
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 if !plus => Some(format!("+1{digits}")),
        11 if digits.starts_with('1') => Some(format!("+{digits}")),
        8..=15 if plus => Some(format!("+{digits}")),
        3..=7 if bare => Some(digits),
        _ => None,
    }
}

/// `number` as it reads: `(425) 555-0199` for a North American one, else as kept.
pub fn shown(number: &str) -> String {
    match ten_digits(number) {
        Some(ten) => format!("({}) {}-{}", &ten[..3], &ten[3..6], &ten[6..]),
        None => number.to_owned(),
    }
}

/// A number's ten digits, as Routes keeps a work phone's: only a North American one, `+1` and
/// ten digits, can be one.
pub fn ten_digits(number: &str) -> Option<String> {
    let ten = number.strip_prefix("+1")?;
    (ten.len() == 10 && ten.bytes().all(|b| b.is_ascii_digit())).then(|| ten.to_owned())
}
