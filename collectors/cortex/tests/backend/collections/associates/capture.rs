use super::*;
use serde_json::json;

#[test]
fn roster_rows_read_into_bounded_associates() {
    let row = json!({
        "transporter_id": "A1B2C3D4E5F6G7",
        "full_name": "  Jordan   Ellis ",
        "email_address": " Jordan.Ellis@Example.com ",
        "personal_phone_number": "+16125550101",
        "work_phone_number": "",
        "operational_status": "ACTIVE",
        "roles": "Driver, Helper, Driver",
        "qualifications": "CDV,, EDV",
        "driver_license_expiration_date": "2027-02-30",
        "photo_url": "/workforce/api/v1/fetchDAPhoto?associate=a1",
    });
    let associate = Associate::read(row.clone()).unwrap().unwrap();
    assert_eq!(associate.name, "Jordan Ellis");
    assert_eq!(associate.email.as_deref(), Some("jordan.ellis@example.com"));
    assert_eq!(associate.personal_phone.as_deref(), Some("+16125550101"));
    assert_eq!(associate.work_phone, None);
    assert_eq!(associate.roles, ["Driver", "Helper"]);
    assert_eq!(associate.qualifications, ["CDV", "EDV"]);
    // No such day.
    assert_eq!(associate.license_expires, None);
    assert_eq!(
        associate.photo.as_deref(),
        Some("/workforce/api/v1/fetchDAPhoto?associate=a1")
    );
    assert_eq!(associate.row, row);
    let with = |key: &str, value: serde_json::Value| {
        let mut row = row.clone();
        row[key] = value;
        Associate::read(row)
    };
    // A photo anywhere but the application's own host is not followed.
    for elsewhere in [
        "https://example.com/photo.jpg",
        "//example.com/photo.jpg",
        "/workforce/../photo",
    ] {
        assert_eq!(with("photo_url", json!(elsewhere)).unwrap().unwrap().photo, None);
    }
    assert_eq!(with("email_address", json!("not an address")).unwrap().unwrap().email, None);
    // A row naming no one Amazon's other pages could name is passed over; one without a name
    // or a status means the page changed.
    assert!(with("transporter_id", json!("")).unwrap().is_none());
    assert!(with("full_name", json!(null)).is_err());
    assert!(with("operational_status", json!("active")).is_err());
}

#[test]
fn a_photo_is_kept_only_as_an_image_whose_bytes_match_its_digest() {
    let png = b"\x89PNG\r\n\x1a\n rest of the image";
    assert!(Photo::new("/workforce/p", b"<html>signed out</html>").is_none());
    let photo = Photo::new("/workforce/p", png).unwrap();
    assert_eq!(photo.format, PhotoFormat::Png);
    assert_eq!(photo.bytes().unwrap(), png);
    let mut changed = photo.clone();
    changed.data = Photo::new("/workforce/p", b"\x89PNG\r\n\x1a\n another image")
        .unwrap()
        .data;
    assert!(changed.bytes().is_err());
}
