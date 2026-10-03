#[test]
fn path_segments_are_decoded() {
    assert_eq!(super::decoded("Daniel%20Ortiz"), "Daniel Ortiz");
    assert_eq!(super::decoded("Jos%C3%A9"), "José");
    assert_eq!(super::decoded("100%"), "100%");
    assert_eq!(super::decoded("K4M7QZ"), "K4M7QZ");
}
