use super::encoding_quality;
#[test]
fn accepts_encodings_and_respects_explicit_opt_outs() {
    assert_eq!(encoding_quality("gzip, br", "br"), 1.0);
    assert_eq!(encoding_quality("gzip;q=0.5, br;q=0.8", "br"), 0.8);
    assert_eq!(encoding_quality("*;q=1, br;q=0", "br"), 0.0);
    assert_eq!(encoding_quality("*;q=0.5", "gzip"), 0.5);
    assert_eq!(encoding_quality("gzip;q=invalid", "gzip"), 0.0);
    assert_eq!(encoding_quality("", "gzip"), 0.0);
}
