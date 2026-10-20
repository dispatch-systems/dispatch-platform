//! Finding people by name, misspelt, with the letters that matched.
use super::*;

/// The names `typed` finds among `names`, the closest first, each with its letters that matched.
fn found<'a>(typed: &str, names: &[&'a str]) -> Vec<(&'a str, Vec<&'a str>)> {
    let mut found: Vec<_> = names
        .iter()
        .filter_map(|name| matched(typed, name).map(|(score, letters)| (score, *name, letters)))
        .collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found
        .into_iter()
        .map(|(_, name, letters)| {
            let units: Vec<u16> = name.encode_utf16().collect();
            let marked = letters
                .iter()
                .map(|&(start, end)| {
                    let part = String::from_utf16(&units[start as usize..end as usize]).unwrap();
                    &name[name.find(&part).unwrap()..][..part.len()]
                })
                .collect();
            (name, marked)
        })
        .collect()
}

#[test]
fn a_misspelt_name_finds_the_closest_first_marking_the_letters_that_matched() {
    let names = ["Jordan Lee", "William James", "Jamar Wilson", "Jamal Williams", "Sofia Ramirez"];
    let results = found("jamel willams", &names);
    assert_eq!(
        results.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["Jamal Williams", "William James", "Jamar Wilson"]
    );
    assert_eq!(results[0].1, ["Jam", "l", "Will", "ams"]);
    // Typed from the start, a word is marked as far as it's typed; case doesn't count.
    assert_eq!(found("SOF", &names), [("Sofia Ramirez", vec!["Sof"])]);
    assert!(found("zzz", &names).is_empty());
    assert!(found("  ", &names).is_empty());
}

#[test]
fn accents_dont_count_and_marks_count_as_the_page_reads_a_string() {
    let names = ["José Núñez", "👋 Ana Silva"];
    assert_eq!(found("jose nunez", &names[..1]), [("José Núñez", vec!["José", "Núñez"])]);
    // An emoji is two of a string's units in the page.
    assert_eq!(matched("ana", names[1]).unwrap().1, [(3, 6)]);
}
