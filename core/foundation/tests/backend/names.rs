use super::*;

#[test]
fn capitals_become_ordinary_case_and_other_spellings_stay() {
    assert_eq!(display("REYES, ANTONIO"), "Antonio Reyes");
    assert_eq!(display("O'NEILL-PRICE, JAMIE"), "Jamie O'Neill-Price");
    assert_eq!(display("McDonald, Ana Sofía"), "Ana Sofía McDonald");
    assert_eq!(display("Tony  Reyes"), "Tony Reyes");
}
