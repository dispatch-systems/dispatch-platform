//! A code reads the same however it's typed, and only a code of its letters is one.
use super::{kept, new_code};

#[test]
fn a_code_is_kept_however_it_is_typed() {
    let code = new_code().unwrap();
    let typed = format!("{}-{}", &code[..4], &code[4..]).to_lowercase();
    assert_eq!(kept(&typed), kept(&code));
    assert!(kept(&code).is_some());
    assert_eq!(kept("O0I1O0I1"), None, "letters a code never has");
    assert_eq!(kept("ABCD"), None, "too short");
}
