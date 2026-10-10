use super::*;

#[test]
fn keys_carry_their_environment_and_a_checksum() {
    let key = new(Kind::Key, Environment::Production).unwrap();
    assert!(key.starts_with("dsk_live_") && key.len() == 9 + RANDOM + CHECK);
    assert!(well_formed(&key, Kind::Key, Environment::Production));
    // A key from one environment never passes for the other's.
    assert!(!well_formed(&key, Kind::Key, Environment::Preview));
    let dev = new(Kind::Key, Environment::Preview).unwrap();
    assert!(dev.starts_with("dsk_dev_") && well_formed(&dev, Kind::Key, Environment::Preview));
    // One changed character fails the checksum.
    let mut typo = key.clone().into_bytes();
    typo[12] = if typo[12] == b'a' { b'b' } else { b'a' };
    assert!(!well_formed(
        &String::from_utf8(typo).unwrap(),
        Kind::Key,
        Environment::Production
    ));
    assert!(!well_formed(
        "dsk_live_short",
        Kind::Key,
        Environment::Production
    ));
    assert_ne!(key, new(Kind::Key, Environment::Production).unwrap());
}
#[test]
fn app_tokens_never_pass_for_one_another_or_for_a_key() {
    let access = new(Kind::Access, Environment::Preview).unwrap();
    let refresh = new(Kind::Refresh, Environment::Preview).unwrap();
    assert!(access.starts_with("dsa_dev_") && refresh.starts_with("dsr_dev_"));
    assert!(well_formed(&access, Kind::Access, Environment::Preview));
    assert!(well_formed(&refresh, Kind::Refresh, Environment::Preview));
    for (token, kind) in [
        (&access, Kind::Refresh),
        (&access, Kind::Key),
        (&refresh, Kind::Access),
    ] {
        assert!(!well_formed(token, kind, Environment::Preview));
    }
    let live = new(Kind::Access, Environment::Production).unwrap();
    assert!(live.starts_with("dsa_live_"));
    assert!(!well_formed(&live, Kind::Access, Environment::Preview));
}
#[test]
fn crc32_is_the_standard_one() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}
