use super::*;

/// EXIF with one entry, the orientation, as a TIFF block in either byte order.
fn exif(orientation: u16, little: bool) -> Vec<u8> {
    let (u16b, u32b): (fn(u16) -> [u8; 2], fn(u32) -> [u8; 4]) = if little {
        (u16::to_le_bytes, u32::to_le_bytes)
    } else {
        (u16::to_be_bytes, u32::to_be_bytes)
    };
    let mut tiff = if little {
        b"II".to_vec()
    } else {
        b"MM".to_vec()
    };
    tiff.extend(u16b(42));
    tiff.extend(u32b(8));
    tiff.extend(u16b(1));
    // Tag 0x0112, a SHORT, one of it, its value padded to four bytes.
    tiff.extend(u16b(0x0112));
    tiff.extend(u16b(3));
    tiff.extend(u32b(1));
    tiff.extend(u16b(orientation));
    tiff.extend([0, 0]);
    tiff.extend(u32b(0));
    tiff
}
/// A camera's photo, `width` by `height`: red in its top-left quarter and blue elsewhere, as a
/// JPEG whose EXIF says to turn it `orientation`.
fn camera(width: u16, height: u16, orientation: Option<u16>) -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let red = x < width / 2 && y < height / 2;
            pixels.extend(if red { [220, 30, 30] } else { [30, 30, 220] });
        }
    }
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, 92);
    if let Some(orientation) = orientation {
        let mut segment = b"Exif\0\0".to_vec();
        segment.extend(exif(orientation, false));
        encoder.add_app_segment(1, segment).unwrap();
    }
    encoder
        .encode(&pixels, width, height, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    out
}
/// A JPEG read back: its size, its pixels and whether it carries EXIF.
fn read(bytes: &[u8]) -> (u16, u16, Vec<u8>, bool) {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    let pixels = decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    (
        info.width,
        info.height,
        pixels,
        decoder.exif_data().is_some(),
    )
}
/// The colour at `x`, `y` of a picture `width` wide, three bytes a pixel.
fn at(pixels: &[u8], width: u16, x: u16, y: u16) -> [u8; 3] {
    let i = (usize::from(y) * usize::from(width) + usize::from(x)) * 3;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}
fn red(rgb: [u8; 3]) -> bool {
    rgb[0] > 150 && rgb[2] < 100
}

#[test]
fn a_camera_photo_comes_out_upright_at_most_400_pixels_as_a_small_jpeg_without_its_exif() {
    let photo = camera(1600, 1200, Some(6));
    let shrunk = shrink(photo).unwrap();
    assert!(shrunk.len() < 60 * 1024, "{} bytes", shrunk.len());
    let (width, height, pixels, exif) = read(&shrunk);
    // Turned a quarter clockwise, as its EXIF said: on its side, its red corner now top right.
    assert_eq!((width, height), (300, 400));
    assert!(red(at(&pixels, width, 225, 100)));
    assert!(!red(at(&pixels, width, 75, 100)));
    assert!(!exif);
}

#[test]
fn a_png_is_shrunk_on_white_where_it_is_see_through() {
    // Opaque blue on the left half, nothing on the right.
    let (width, height) = (800u32, 600u32);
    let mut pixels = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            pixels.extend(if x < width / 2 {
                [30, 30, 220, 255]
            } else {
                [0, 0, 0, 0]
            });
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
    }
    let (width, height, pixels, _) = read(&shrink(png).unwrap());
    assert_eq!((width, height), (400, 300));
    assert!(at(&pixels, width, 100, 150)[2] > 150);
    assert!(
        at(&pixels, width, 300, 150)
            .iter()
            .all(|channel| *channel > 240)
    );
}

#[test]
fn a_photo_already_small_is_kept_byte_for_byte() {
    // As small as one shrunk, or within the size kept and as light as one.
    let small = camera(320, 240, Some(6));
    assert_eq!(shrink(small.clone()).unwrap(), small);
    let heavy = camera(400, 300, None);
    assert!(heavy.len() <= MAX_KEPT_PHOTO_BYTES);
    assert_eq!(shrink(heavy.clone()).unwrap(), heavy);
    // Nothing here reads a WebP; it is kept as it came.
    let webp = b"RIFF\0\0\0\0WEBPVP8 rest".to_vec();
    assert_eq!(shrink(webp.clone()).unwrap(), webp);
}

#[test]
fn what_is_no_image_cannot_be_read_or_claims_too_many_pixels_is_refused() {
    assert_eq!(
        shrink(b"<html>a page</html>".to_vec()).unwrap_err(),
        "photo_not_an_image"
    );
    assert_eq!(
        shrink(b"\xFF\xD8\xFF\xE0 not a jpeg".to_vec()).unwrap_err(),
        "photo_unreadable"
    );
    // A PNG claiming ten thousand pixels square, refused before a pixel is read.
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 10_000, 10_000);
        encoder.set_color(png::ColorType::Rgb);
        let mut writer = encoder.write_header().unwrap();
        writer.write_chunk(png::chunk::IDAT, &[0; 16]).unwrap();
    }
    assert_eq!(shrink(png).unwrap_err(), "photo_too_large");
}

#[test]
fn each_exif_orientation_stands_the_picture_upright() {
    // a b c
    // d e f
    let picture = |orientation| Picture {
        width: 3,
        height: 2,
        channels: 1,
        pixels: b"abcdef".to_vec(),
        orientation,
    };
    let upright = |orientation| {
        let p = picture(orientation).upright();
        (p.width, p.height, String::from_utf8(p.pixels).unwrap())
    };
    let expected = [
        (1, (3, 2, "abcdef")),
        (2, (3, 2, "cbafed")),
        (3, (3, 2, "fedcba")),
        (4, (3, 2, "defabc")),
        (5, (2, 3, "adbecf")),
        (6, (2, 3, "daebfc")),
        (7, (2, 3, "fcebda")),
        (8, (2, 3, "cfbead")),
    ];
    for (orientation, (width, height, pixels)) in expected {
        assert_eq!(
            upright(orientation),
            (width, height, pixels.to_owned()),
            "orientation {orientation}"
        );
    }
    // Read from EXIF in either byte order; one out of range, or none, is upright.
    assert_eq!(orientation(&exif(8, true)), 8);
    assert_eq!(orientation(&exif(3, false)), 3);
    assert_eq!(orientation(&exif(9, true)), 1);
    assert_eq!(orientation(b"II\x2a\0"), 1);
}
