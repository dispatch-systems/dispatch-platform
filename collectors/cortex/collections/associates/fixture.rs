//! What fixture mode collects: a small made-up roster, named as the demo's Paycom employees
//! are, some by another spelling, with portraits drawn here.
use super::capture::{Associate, Capture, Collection, Photo, Request};
use dispatch_core::{Error, Result, db::now};
use serde_json::json;

/// Transporter ID, name as Amazon writes it, status and roles.
const ROSTER: [(&str, &str, &str, &str); 8] = [
    ("driver-1", "Jordan Ellis", "ACTIVE", "Driver, Helper"),
    ("driver-2", "Morgan Reed", "ACTIVE", "Driver, Helper"),
    ("driver-3", "Taylor Brooks", "ACTIVE", "Driver"),
    ("driver-4", "Cam Hayes", "ACTIVE", "Driver, Helper"),
    ("driver-5", "Casey Rivera", "ACTIVE", "Helper"),
    ("driver-6", "Riley Bennett", "INACTIVE", "Driver"),
    ("driver-7", "Dana Whitfield", "ACTIVE", "Driver"),
    ("driver-8", "Alex Parker", "ACTIVE", "Driver, Helper"),
];

pub fn fixture(request: &Request) -> Result<Capture> {
    request.validate()?;
    let started_at = now();
    let mut associates = Vec::new();
    let mut photos = Vec::new();
    for (index, (id, name, status, roles)) in ROSTER.iter().enumerate() {
        let address = format!("/workforce/api/v1/fetchDAPhoto?associate={id}");
        let email = format!("{}@example.com", name.to_lowercase().replace(' ', "."));
        let row = json!({
            "transporter_id": id,
            "full_name": name,
            "email_address": email,
            "personal_phone_number": format!("+1612555{:04}", 100 + index),
            "work_phone_number": if index % 3 == 0 { String::new() } else { format!("+1651555{:04}", 100 + index) },
            "operational_status": status,
            "roles": roles,
            "qualifications": if index % 2 == 0 { "CDV, EDV" } else { "CDV" },
            "driver_license_expiration_date": format!("2027-0{}-15", 1 + index % 9),
            "provider_id": "provider-fixture",
            "photo_url": address,
        });
        let associate =
            Associate::read(row)?.ok_or_else(|| Error::new("associates_row_invalid", 502))?;
        photos.extend(Photo::new(&address, &portrait(index)));
        associates.push(associate);
    }
    let capture = Capture {
        version: 1,
        collection: Collection::Associates,
        company_id: "company-fixture".into(),
        company_code: request.dsp_abbreviation.clone(),
        started_at,
        finished_at: now().max(started_at),
        associates,
        photos,
        photos_waiting: 0,
    };
    capture.validate(request)?;
    Ok(capture)
}

/// The tones portraits are drawn on, top to bottom.
const TONES: [([u8; 3], [u8; 3]); 5] = [
    ([199, 211, 230], [127, 149, 182]),
    ([217, 207, 232], [154, 135, 191]),
    ([205, 227, 216], [127, 174, 152]),
    ([232, 217, 203], [184, 151, 124]),
    ([208, 221, 228], [134, 163, 179]),
];
/// A made-up portrait: a pale head and shoulders on a tone, as a PNG.
fn portrait(index: usize) -> Vec<u8> {
    const SIZE: u32 = 96;
    let (top, bottom) = TONES[index % TONES.len()];
    // A head of its own size, so no two portraits are the same file.
    let radius = 15.0 + (index % 4) as f32;
    let mut raw = Vec::with_capacity(((SIZE * 3 + 1) * SIZE) as usize);
    for y in 0..SIZE {
        // Each row starts with its filter: none.
        raw.push(0);
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 - 48.0, y as f32);
            let head = dx * dx + (dy - 38.0) * (dy - 38.0) <= radius * radius;
            let shoulders =
                dy >= 62.0 && (dx * dx) / (34.0 * 34.0) + (dy - 100.0) * (dy - 100.0) / (38.0 * 38.0) <= 1.0;
            let t = dy / SIZE as f32;
            for channel in 0..3 {
                let tone = f32::from(top[channel]) * (1.0 - t) + f32::from(bottom[channel]) * t;
                let value = if head || shoulders { tone * 0.2 + 255.0 * 0.8 } else { tone };
                raw.push(value.round() as u8);
            }
        }
    }
    let mut header = Vec::new();
    header.extend(SIZE.to_be_bytes());
    header.extend(SIZE.to_be_bytes());
    // Eight bits a channel, RGB, no interlacing.
    header.extend([8, 2, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib(&raw));
    chunk(&mut png, b"IEND", &[]);
    png
}
fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend((data.len() as u32).to_be_bytes());
    png.extend(kind);
    png.extend(data);
    png.extend(crc32(&[kind.as_slice(), data].concat()).to_be_bytes());
}
/// `data` in zlib's format, stored without compression.
fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = data.chunks(65_535).collect();
    for (index, block) in blocks.iter().enumerate() {
        out.push(u8::from(index + 1 == blocks.len()));
        let length = block.len() as u16;
        out.extend(length.to_le_bytes());
        out.extend((!length).to_le_bytes());
        out.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    out.extend(((b << 16) | a).to_be_bytes());
    out
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
