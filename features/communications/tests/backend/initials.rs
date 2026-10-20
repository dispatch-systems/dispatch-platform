//! The image of someone's initials Slack shows their messages under: the two letters, from their
//! first and last names, and nothing else.
use super::*;
use std::io::Read;

#[test]
fn someones_initials_come_from_their_first_and_last_names() {
    assert_eq!(of("Keisha", "Brown").as_deref(), Some("KB"));
    assert_eq!(of("josé", "Núñez").as_deref(), Some("JN"));
    assert_eq!(of("  Cher", "").as_deref(), Some("C"));
    assert_eq!(of("李", "王"), None);
    assert_eq!(
        address("https://admin.example.test", "KB"),
        "https://admin.example.test/communications/initials/1/KB.png"
    );
}

#[test]
fn an_image_shows_one_or_two_capitals_drawn_on_their_tone() {
    for refused in ["kb.png", "KBC.png", "K1.png", "KB.jpg", "KB", ".png", "../KB.png"] {
        assert!(image(refused).is_none(), "{refused}");
    }
    let png = image("KB.png").unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    // Its pixels, from its one compressed chunk: unfiltered rows of red, green and blue.
    let length = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
    assert_eq!(&png[37..41], b"IDAT");
    let mut rows = Vec::new();
    flate2::read::ZlibDecoder::new(&png[41..41 + length])
        .read_to_end(&mut rows)
        .unwrap();
    let pixels: Vec<&[u8]> = rows
        .chunks(SIZE * 3 + 1)
        .flat_map(|row| row[1..].chunks(3))
        .collect();
    assert_eq!(pixels.len(), SIZE * SIZE);
    // Its corners are its tone's surface; its letters, in the middle, its ink.
    let (ink, surface) = TONES.iter().find(|(_, surface)| pixels[0] == surface).unwrap();
    assert!(pixels.iter().any(|pixel| pixel == ink));
    let inked = pixels.iter().filter(|pixel| *pixel != surface).count();
    assert!((SIZE * SIZE / 50..SIZE * SIZE / 4).contains(&inked), "{inked}");
}
