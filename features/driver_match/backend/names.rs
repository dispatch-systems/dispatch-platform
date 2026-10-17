//! How Driver Match compares names. Sources format one name differently: "REYES, ANTONIO" in
//! Paycom, "Antonio Reyes" in Amazon's routes, sometimes with a middle name, a suffix or
//! without accents. Comparisons ignore case, accents, punctuation and order, never spelling.

/// A letter without its accent, for the Latin letters names use.
fn plain(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
        'ł' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' => 's',
        'ť' | 'ţ' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

/// A name as it is said, first name first: "Last, First" put in order.
fn in_order(name: &str) -> std::borrow::Cow<'_, str> {
    match name.split_once(',') {
        Some((last, first)) => format!("{first} {last}").into(),
        None => name.into(),
    }
}

/// A name with its formatting removed: in order, then only its letters and digits,
/// lowercased and without accents. Names with one key are one name.
pub fn name_key(name: &str) -> String {
    in_order(name)
        .to_lowercase()
        .chars()
        .map(plain)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// A name in parts: the first name, the rest of the words, and a suffix.
pub struct Name {
    pub given: String,
    pub surnames: Vec<String>,
    pub suffix: Option<String>,
}

impl Name {
    pub fn new(name: &str) -> Self {
        let mut words: Vec<_> = in_order(name)
            .split_whitespace()
            .map(name_key)
            .filter(|word| !word.is_empty())
            .collect();
        // A suffix closes a name, though the scorecard sometimes writes it before the last
        // name, as "Enrique Henry Jr Cortez". The first word is always the first name.
        let mut suffix = None;
        let mut index = 1;
        while index < words.len() {
            let found = match words[index].as_str() {
                "jr" | "junior" => Some("jr"),
                "sr" | "senior" => Some("sr"),
                "ii" => Some("ii"),
                "iii" => Some("iii"),
                "iv" => Some("iv"),
                _ => None,
            };
            if let Some(found) = found {
                suffix = Some(found.to_owned());
                words.remove(index);
            } else {
                index += 1;
            }
        }
        let given = if words.is_empty() {
            String::new()
        } else {
            words.remove(0)
        };
        Self {
            given,
            surnames: words,
            suffix,
        }
    }

    /// Whether two names are one name written two ways, with the same suffix if both
    /// have one: the same first name with one surname the whole of the other's or its
    /// first words, or the same last name with one first name the other's first or middle
    /// name.
    pub fn matches(&self, other: &Self) -> bool {
        if self.given.is_empty()
            || other.given.is_empty()
            || self.surnames.is_empty()
            || other.surnames.is_empty()
            || (self.suffix.is_some() && other.suffix.is_some() && self.suffix != other.suffix)
        {
            return false;
        }
        // Someone may go by a middle name: "Martin Ruben Morgan" is "MORGAN, RUBEN".
        let middle =
            |name: &Self, first: &String| name.surnames[..name.surnames.len() - 1].contains(first);
        if self.same_last(other)
            && (self.given == other.given
                || middle(self, &other.given)
                || middle(other, &self.given))
        {
            return true;
        }
        if !self.same_given(other) {
            return false;
        }
        // One provider may omit a second surname or join surname words. Require
        // the entire shorter surname at a word boundary, not a fuzzy substring.
        let prefix = |short: &[String], long: &[String]| {
            let short = short.concat();
            let mut joined = String::new();
            long.iter().any(|word| {
                joined.push_str(word);
                joined == short
            })
        };
        prefix(&self.surnames, &other.surnames) || prefix(&other.surnames, &self.surnames)
    }

    /// The last word of the surname: what "the same last name" compares.
    pub fn last(&self) -> Option<&str> {
        self.surnames.last().map(String::as_str)
    }
    /// Whether two names share a first name.
    pub fn same_given(&self, other: &Self) -> bool {
        !self.given.is_empty() && self.given == other.given
    }
    /// Whether two names share a last name longer than an initial.
    pub fn same_last(&self, other: &Self) -> bool {
        self.last()
            .is_some_and(|last| last.chars().count() > 1 && other.last() == Some(last))
    }
}

/// Every key of a list of names, without the empty ones.
fn keys(names: &[String]) -> std::collections::BTreeSet<String> {
    names
        .iter()
        .map(|n| name_key(n))
        .filter(|k| !k.is_empty())
        .collect()
}
/// Whether two lists of one person's names share a name, however it is formatted.
pub fn exact(a: &[String], b: &[String]) -> bool {
    !keys(a).is_disjoint(&keys(b))
}
/// Whether two lists of one person's names share a name written two ways (`Name::matches`).
pub fn variant(a: &[String], b: &[String]) -> bool {
    a.iter()
        .any(|x| b.iter().any(|y| Name::new(x).matches(&Name::new(y))))
}
/// Whether two lists of names could be one person's: a name in common, or a first or last
/// name, so a contact two people share joins them only when their names agree.
pub fn agree(a: &[String], b: &[String]) -> bool {
    exact(a, b)
        || a.iter().any(|x| {
            let x = Name::new(x);
            b.iter().any(|y| {
                let y = Name::new(y);
                x.same_given(&y) || x.same_last(&y)
            })
        })
}

/// First names people go by in place of their own, each beside the names it shortens.
/// A name that is the start of another ("Chris", "Christopher") needs no entry.
const SHORT_FORMS: &[(&str, &[&str])] = &[
    ("tony", &["antonio", "anthony"]),
    ("toni", &["antonia"]),
    ("mike", &["michael", "miguel"]),
    ("bob", &["robert"]),
    ("bobby", &["robert"]),
    ("rob", &["robert", "roberto"]),
    ("beto", &["roberto", "alberto"]),
    ("bill", &["william"]),
    ("billy", &["william"]),
    ("will", &["william", "wilfredo"]),
    ("jim", &["james"]),
    ("jimmy", &["james"]),
    ("jack", &["john", "jackson"]),
    ("johnny", &["john", "jonathan"]),
    ("jon", &["jonathan"]),
    ("joe", &["joseph", "jose"]),
    ("joey", &["joseph"]),
    ("pepe", &["jose"]),
    ("dave", &["david"]),
    ("tom", &["thomas", "tomas"]),
    ("tommy", &["thomas"]),
    ("rick", &["richard", "ricardo", "eric"]),
    ("ricky", &["richard", "ricardo"]),
    ("rich", &["richard"]),
    ("dick", &["richard"]),
    ("ed", &["edward", "eduardo", "edwin"]),
    ("eddie", &["edward", "eduardo"]),
    ("lalo", &["eduardo"]),
    ("ted", &["theodore", "edward"]),
    ("andy", &["andrew", "andres"]),
    ("drew", &["andrew"]),
    ("liz", &["elizabeth"]),
    ("beth", &["elizabeth", "bethany"]),
    ("betty", &["elizabeth"]),
    ("kate", &["katherine", "kathryn", "catherine"]),
    ("katie", &["katherine", "kathryn", "catherine"]),
    ("kathy", &["katherine", "kathryn", "kathleen"]),
    ("cathy", &["catherine"]),
    ("peggy", &["margaret"]),
    ("maggie", &["margaret"]),
    ("meg", &["margaret", "megan"]),
    ("chuck", &["charles"]),
    ("charlie", &["charles"]),
    ("hank", &["henry"]),
    ("harry", &["henry", "harold"]),
    ("jake", &["jacob"]),
    ("larry", &["lawrence"]),
    ("manny", &["manuel"]),
    ("chuy", &["jesus"]),
    ("nacho", &["ignacio"]),
    ("paco", &["francisco"]),
    ("pancho", &["francisco"]),
    ("frank", &["francisco", "franklin"]),
    ("memo", &["guillermo"]),
    ("lupe", &["guadalupe"]),
    ("tono", &["antonio"]),
    ("sandy", &["sandra", "alexandra"]),
    ("sasha", &["alexandra", "alexander"]),
    ("shay", &["shanice", "shaniqua", "shayla"]),
    ("nate", &["nathan", "nathaniel"]),
    ("zack", &["zachary"]),
    ("gabe", &["gabriel"]),
    ("gaby", &["gabriela", "gabriella"]),
    ("abby", &["abigail"]),
    ("trish", &["patricia"]),
    ("patty", &["patricia"]),
    ("sue", &["susan", "suzanne"]),
    ("vicky", &["victoria"]),
    ("jen", &["jennifer"]),
    ("jenny", &["jennifer"]),
    ("steve", &["steven", "stephen"]),
    ("greg", &["gregory"]),
    ("jeff", &["jeffrey"]),
    ("ken", &["kenneth"]),
    ("kenny", &["kenneth"]),
    ("tim", &["timothy"]),
    ("matt", &["matthew"]),
    ("nick", &["nicholas", "nicolas"]),
    ("dan", &["daniel"]),
    ("danny", &["daniel"]),
    ("ben", &["benjamin"]),
    ("josh", &["joshua"]),
    ("sam", &["samuel", "samantha"]),
    ("alex", &["alexander", "alexandra", "alejandro", "alexis"]),
    (
        "chris",
        &["christopher", "christian", "christina", "cristian"],
    ),
];

/// When one first name is a short form of the other, the two as (short, long).
pub fn short_form<'a>(a: &'a str, b: &'a str) -> Option<(&'a str, &'a str)> {
    if a.is_empty() || b.is_empty() || a == b {
        return None;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let listed = SHORT_FORMS
        .iter()
        .any(|(s, longs)| *s == short && longs.contains(&long));
    // The start of the longer name counts from three letters: "Chris" and "Christopher",
    // never "Al" and "Alberto".
    (listed || (short.chars().count() >= 3 && long.starts_with(short))).then_some((short, long))
}

/// A first name with its first letter in capitals, for evidence such as "Tony is short
/// for Antonio".
pub fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../tests/backend/names.rs"]
mod tests;
