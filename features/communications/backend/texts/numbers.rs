//! Phone numbers, each written one way as `texting` writes them (its `normal`): how one reads
//! (`shown`); the ten digits Routes keeps a work phone by (`ten_digits`), and back
//! (`work_number`); and what the DSP keeps of the numbers that text it, and what its phones are
//! called, to read its texts by (`Numbers`).
use super::storage::ContactStore;
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

/// `number` as it reads: `(425) 555-0199` for a North American one, else as kept.
pub fn shown(number: &str) -> String {
    match ten_digits(number) {
        Some(ten) => format!("({}) {}-{}", &ten[..3], &ten[3..6], &ten[6..]),
        None => number.to_owned(),
    }
}

/// A work phone's number as texts write it, from its ten digits as Routes keeps it.
pub fn work_number(ten: &str) -> String {
    format!("+1{ten}")
}

/// A number's ten digits, as Routes keeps a work phone's: only a North American one, `+1` and
/// ten digits, can be one.
pub fn ten_digits(number: &str) -> Option<String> {
    let ten = number.strip_prefix("+1")?;
    (ten.len() == 10 && ten.bytes().all(|b| b.is_ascii_digit())).then(|| ten.to_owned())
}
