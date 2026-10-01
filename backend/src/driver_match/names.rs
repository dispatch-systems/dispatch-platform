//! How names are compared. Sources format one name differently: "REYES, ANTONIO" in
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

/// A name with its formatting removed: "Last, First" put in order, then only its letters
/// and digits, lowercased and without accents. Names with one key are one name.
pub(crate) fn name_key(name: &str) -> String {
    let ordered = name
        .split_once(',')
        .map(|(last, first)| format!("{first} {last}"));
    ordered
        .as_deref()
        .unwrap_or(name)
        .to_lowercase()
        .chars()
        .map(plain)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// A name in parts: the first name, the rest of the words, and a closing suffix.
pub(crate) struct Name {
    pub given: String,
    pub surnames: Vec<String>,
    pub suffix: Option<String>,
}

impl Name {
    pub fn new(name: &str) -> Self {
        let ordered = name
            .split_once(',')
            .map(|(last, first)| format!("{first} {last}"));
        let mut words: Vec<_> = ordered
            .as_deref()
            .unwrap_or(name)
            .split_whitespace()
            .map(name_key)
            .filter(|word| !word.is_empty())
            .collect();
        let suffix = words.last().and_then(|word| match word.as_str() {
            "jr" | "junior" => Some("jr"),
            "sr" | "senior" => Some("sr"),
            "ii" => Some("ii"),
            "iii" => Some("iii"),
            "iv" => Some("iv"),
            _ => None,
        });
        let suffix = suffix.map(str::to_owned);
        if suffix.is_some() {
            words.pop();
        }
        let given = if words.is_empty() {
            String::new()
        } else {
            words.remove(0)
        };
        // Supported short forms are explicit, never arbitrary first-name prefixes
        // (e.g. Alex must not also match Alexis or Alexandra).
        let given = if given == "alex" {
            "alexander".into()
        } else {
            given
        };
        Self {
            given,
            surnames: words,
            suffix,
        }
    }

    /// Whether two names are one name written two ways: the same first name and suffix,
    /// with one surname the whole of the other's, or its first words.
    pub fn matches(&self, other: &Self) -> bool {
        if self.given.is_empty()
            || self.given != other.given
            || self.surnames.is_empty()
            || other.surnames.is_empty()
            || (self.suffix.is_some() && other.suffix.is_some() && self.suffix != other.suffix)
        {
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
pub(crate) fn short_form<'a>(a: &'a str, b: &'a str) -> Option<(&'a str, &'a str)> {
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

/// A name as people read it: "First Last". A source that writes only capitals, as Paycom
/// does, is put in ordinary case; any other spelling is kept as written.
pub(crate) fn display(name: &str) -> String {
    let ordered = name
        .split_once(',')
        .map(|(last, first)| format!("{} {}", first.trim(), last.trim()))
        .unwrap_or_else(|| name.trim().to_owned());
    let words: Vec<&str> = ordered.split_whitespace().collect();
    if ordered.chars().any(char::is_lowercase) {
        return words.join(" ");
    }
    words
        .iter()
        .map(|word| {
            let mut out = String::with_capacity(word.len());
            let mut start = true;
            for c in word.chars() {
                if start {
                    out.extend(c.to_uppercase());
                } else {
                    out.extend(c.to_lowercase());
                }
                start = matches!(c, '-' | '\'' | '’');
            }
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}
/// A first name with its first letter in capitals, for evidence such as "Tony is short
/// for Antonio".
pub(crate) fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_order_case_punctuation_and_accents() {
        assert_eq!(name_key("REYES, ANTONIO"), name_key("Antonio Reyes"));
        assert_eq!(name_key("O’Neill, Jamie"), name_key("JAMIE O'NEILL"));
        assert_eq!(name_key("Sánchez, René"), name_key("Rene Sanchez"));
        assert_eq!(
            name_key("MOLINA REED, TAYLOR"),
            name_key("Taylor MolinaReed")
        );
        assert_ne!(name_key("Alex Reed"), name_key("Alexander Reed"));
    }
    #[test]
    fn short_forms_come_from_the_list_or_the_start_of_a_name() {
        assert_eq!(short_form("tony", "antonio"), Some(("tony", "antonio")));
        assert_eq!(
            short_form("christopher", "chris"),
            Some(("chris", "christopher"))
        );
        assert_eq!(short_form("al", "alberto"), None);
        assert_eq!(short_form("maria", "mario"), None);
        assert_eq!(short_form("sam", "sam"), None);
    }
    #[test]
    fn capitals_become_ordinary_case_and_other_spellings_stay() {
        assert_eq!(display("REYES, ANTONIO"), "Antonio Reyes");
        assert_eq!(display("O'NEILL-PRICE, JAMIE"), "Jamie O'Neill-Price");
        assert_eq!(display("McDonald, Ana Sofía"), "Ana Sofía McDonald");
        assert_eq!(display("Tony  Reyes"), "Tony Reyes");
    }
    #[test]
    fn a_surname_counts_its_last_word() {
        assert_eq!(Name::new("HERNANDEZ ORTIZ, LUIS").last(), Some("ortiz"));
        assert_eq!(Name::new("James Whitfield Jr").last(), Some("whitfield"));
        assert_eq!(Name::new("Jordan K.").last(), Some("k"));
        assert_eq!(Name::new("Prince").last(), None);
    }
}
