use super::*;

#[test]
fn capitals_become_ordinary_case_and_other_spellings_stay() {
    assert_eq!(display("REYES, ANTONIO"), "Antonio Reyes");
    assert_eq!(display("O'NEILL-PRICE, JAMIE"), "Jamie O'Neill-Price");
    assert_eq!(display("McDonald, Ana Sofía"), "Ana Sofía McDonald");
    assert_eq!(display("Tony  Reyes"), "Tony Reyes");
}

#[test]
fn a_name_is_named_by_a_name_part_of_one_or_an_initial() {
    assert!(matches("Avery Morgan", "Avery M."));
    assert!(matches("avery m", "Avery Morgan"));
    assert!(matches("Morgan", "Martin Morgan"));
    // A joined surname holds both of its names.
    assert!(matches("Molinero", "Bryan MolineroMartinez"));
    assert!(matches("Bryan Martinez", "Bryan MolineroMartinez"));
    assert!(!matches("Ann", "Andrew Trujillo"));
    assert!(!matches("Avery Avery", "Avery Morgan"));
    assert!(!matches("", "Avery Morgan"));
    assert!(same("avery morgan", "Avery Morgan."));
    assert!(!same("Avery", "Avery Morgan"));
    assert!(!same("", ""));
}
