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
fn a_first_or_middle_name_with_the_same_last_name_matches() {
    let same = |a: &str, b: &str| Name::new(a).matches(&Name::new(b));
    assert!(same("MORGAN, RUBEN", "Martin Ruben Morgan"));
    assert!(same("BELL, MARCUS", "Marcus Andre Bell"));
    assert!(same("CORTEZ, HENRY", "Enrique Henry Jr Cortez"));
    assert!(same("SALAZAR, ALEX", "Alexander Raymond Salazar"));
    assert!(same("HERNANDEZ ORTIZ, LUIS", "Luis Hernandez"));
    // A middle name alone never stands in for a last name, an initial or a suffix.
    assert!(!same("CORTEZ, HENRY", "Henry Vincent Flores"));
    assert!(!same("MORGAN, RUBEN", "Ruben Martin"));
    assert!(!same("KOWALSKI, JORDAN", "Avery Jordan K."));
    assert!(!same("WHITFIELD JR, JAMES", "James Edward Whitfield Sr"));
    assert!(same("WHITFIELD JR, JAMES", "James Edward Whitfield"));
}
#[test]
fn a_suffix_is_found_wherever_it_is_written() {
    let name = Name::new("Enrique Henry Jr Cortez");
    assert_eq!(name.given, "enrique");
    assert_eq!(name.surnames, ["henry", "cortez"]);
    assert_eq!(name.suffix.as_deref(), Some("jr"));
    assert_eq!(Name::new("Junior Garcia").given, "junior");
}
#[test]
fn a_surname_counts_its_last_word() {
    assert_eq!(Name::new("HERNANDEZ ORTIZ, LUIS").last(), Some("ortiz"));
    assert_eq!(Name::new("James Whitfield Jr").last(), Some("whitfield"));
    assert_eq!(Name::new("Jordan K.").last(), Some("k"));
    assert_eq!(Name::new("Prince").last(), None);
}
