//! Finding people by name, misspelt or not, as New message and the list of conversations do, with
//! the letters of each name that matched, for the page to mark. Each word typed is matched to
//! the word of the name it's most like: one it starts, or one sharing its first letter and most
//! of its letters in order. A name is found when its words are, on the whole, like those typed,
//! and the closest come first, a name taking the words in the order typed before one that
//! doesn't. Case and accents don't count.

/// How like a word typed a word of a name must be, and a name the words typed, to be found.
const LIKE: f64 = 0.5;
/// How much more a name counts when its words come in the order typed.
const IN_ORDER: f64 = 0.05;

/// How like `typed` the name `name` is, and its letters that matched: each a range of its
/// UTF-16 code units, as the page reads a string. None when it isn't found.
pub fn matched(typed: &str, name: &str) -> Option<(f64, Vec<(u32, u32)>)> {
    let asked = words(typed);
    if asked.is_empty() {
        return None;
    }
    let named = words(name);
    let mut total = 0.0;
    let mut marked: Vec<usize> = Vec::new();
    let mut order = Vec::new();
    for (_, word) in &asked {
        let best = named
            .iter()
            .enumerate()
            .filter_map(|(at, (start, part))| {
                let (score, letters) = like(word, part)?;
                Some((score, at, letters.into_iter().map(|letter| start + letter).collect()))
            })
            .max_by(|a: &(f64, usize, Vec<usize>), b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
        if let Some((score, at, letters)) = best {
            total += score;
            order.push(at);
            marked.extend(letters);
        }
    }
    let mut score = total / asked.len() as f64;
    if score < LIKE {
        return None;
    }
    if order.windows(2).all(|pair| pair[0] < pair[1]) {
        score += IN_ORDER;
    }
    Some((score, ranges(name, marked)))
}

/// How like the word typed `word` is a word of a name, `part`, and its letters that matched, by
/// their place in it: the start of it, or most of its letters in order, beginning with its first.
fn like(word: &[char], part: &[char]) -> Option<(f64, Vec<usize>)> {
    if part.starts_with(word) {
        return Some((1.0, (0..word.len()).collect()));
    }
    if word.len() < 2 || word.first() != part.first() {
        return None;
    }
    let shared = common(part, word);
    let score = shared.len() as f64 / word.len().max(part.len().min(word.len() + 1)) as f64;
    (shared.len() >= 2 && score >= LIKE).then_some((score, shared))
}

/// The letters of `part` in the longest run it shares with `word`, in order, by their place.
fn common(part: &[char], word: &[char]) -> Vec<usize> {
    let (n, m) = (part.len(), word.len());
    let mut longest = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            longest[i][j] = if part[i] == word[j] {
                longest[i + 1][j + 1] + 1
            } else {
                longest[i + 1][j].max(longest[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut shared) = (0, 0, Vec::new());
    while i < n && j < m {
        if part[i] == word[j] {
            shared.push(i);
            i += 1;
            j += 1;
        } else if longest[i + 1][j] >= longest[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    shared
}

/// The words of `text`, each with the place of its first letter among the text's characters,
/// in lowercase without accents.
fn words(text: &str) -> Vec<(usize, Vec<char>)> {
    let mut found = Vec::new();
    let mut current: Option<(usize, Vec<char>)> = None;
    for (at, letter) in text.chars().enumerate() {
        if letter.is_alphanumeric() {
            current
                .get_or_insert_with(|| (at, Vec::new()))
                .1
                .push(plain(letter));
        } else if let Some(word) = current.take() {
            found.push(word);
        }
    }
    found.extend(current);
    found
}

/// A letter in lowercase, without its accent.
pub(crate) fn plain(letter: char) -> char {
    let lower = letter.to_lowercase().next().unwrap_or(letter);
    match lower {
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

/// The characters of `name` at `marked`, as runs of its UTF-16 code units: start and end.
fn ranges(name: &str, mut marked: Vec<usize>) -> Vec<(u32, u32)> {
    marked.sort_unstable();
    marked.dedup();
    let mut units = Vec::with_capacity(name.len() + 1);
    let mut unit = 0u32;
    for letter in name.chars() {
        units.push(unit);
        unit += letter.len_utf16() as u32;
    }
    units.push(unit);
    let mut found: Vec<(u32, u32)> = Vec::new();
    for at in marked {
        let (start, end) = (units[at], units[at + 1]);
        match found.last_mut() {
            Some(last) if last.1 == start => last.1 = end,
            _ => found.push((start, end)),
        }
    }
    found
}

#[cfg(test)]
#[path = "../tests/backend/search.rs"]
mod tests;
