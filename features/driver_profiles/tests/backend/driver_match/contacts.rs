use super::*;

#[test]
fn contacts_are_written_one_way_or_not_at_all() {
    assert_eq!(
        email(" Jordan.Ellis@Example.com ").as_deref(),
        Some("jordan.ellis@example.com")
    );
    for not_one in [
        "jordan",
        "jordan@localhost",
        "a@b@example.com",
        "jo rdan@example.com",
    ] {
        assert_eq!(email(not_one), None, "{not_one}");
    }
    for written in [
        "+1 (612) 555-0101",
        "612.555.0101",
        "16125550101",
        "6125550101",
    ] {
        assert_eq!(phone(written).as_deref(), Some("6125550101"), "{written}");
    }
    for not_one in [
        "555-0101",
        "+44 20 7946 0958",
        "000-000-0000",
        "111-111-1111",
    ] {
        assert_eq!(phone(not_one), None, "{not_one}");
    }
}
