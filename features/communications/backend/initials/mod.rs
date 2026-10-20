//! The image a message from someone at the DSP goes out under when they have no Slack photo: their
//! first and last initials, as the dashboard draws people without a photo, in Inter at weight
//! 600 on one of its identity tones. Slack shows a message's photo only from a web address, so
//! Dispatch publishes each pair of initials at one of its own, open to anyone, showing the two
//! letters and nothing else. A pair is drawn once and kept for the server's run.
mod glyphs;

use super::search;
use glyphs::{CAP_HEIGHT, LETTERS, UNITS};
use std::{
    collections::HashMap,
    io::Write,
    sync::{Arc, LazyLock, Mutex},
};

/// Where each pair of initials is published, on the admin's address: `<letters>.png`. Its
/// version changes with the drawing, so Slack, which keeps an image by its address, shows a new
/// one.
pub const PATH: &str = "/communications/initials/1/{name}";
/// How many pixels square an image is.
const SIZE: usize = 256;
/// How tall its letters are, to the em, as the dashboard sizes an avatar's.
const EM: f32 = 0.37 * SIZE as f32;
/// The dashboard's identity tones: each its ink and the surface it's on.
const TONES: [([u8; 3], [u8; 3]); 4] = [
    ([0x65, 0x53, 0xaa], [0xf0, 0xed, 0xf9]),
    ([0x36, 0x76, 0x5e], [0xea, 0xf4, 0xee]),
    ([0x51, 0x64, 0x7d], [0xea, 0xf0, 0xf7]),
    ([0x42, 0x73, 0x80], [0xe9, 0xf3, 0xf5]),
];

/// Someone's initials, from their first and last names: the first letter of each, as a capital
/// without its accent. None when neither has a letter Inter's capitals draw.
pub fn of(first: &str, last: &str) -> Option<String> {
    let initial = |name: &str| {
        name.chars()
            .find(|letter| letter.is_alphabetic())
            .map(search::plain)
            .filter(char::is_ascii_lowercase)
            .map(|letter| letter.to_ascii_uppercase())
    };
    let letters: String = [initial(first), initial(last)].into_iter().flatten().collect();
    (!letters.is_empty()).then_some(letters)
}
/// The address of the image of `letters` at `origin`.
pub fn address(origin: &str, letters: &str) -> String {
    format!("{origin}{}", PATH.replace("{name}", &format!("{letters}.png")))
}

/// The image the file name `name` asks for, `<letters>.png`, as a PNG: none for one that isn't
/// one or two capitals.
pub fn image(name: &str) -> Option<Arc<Vec<u8>>> {
    let letters = name.strip_suffix(".png")?;
    let valid = (1..=2).contains(&letters.len()) && letters.bytes().all(|b| b.is_ascii_uppercase());
    if !valid {
        return None;
    }
    static DRAWN: LazyLock<Mutex<HashMap<String, Arc<Vec<u8>>>>> = LazyLock::new(Default::default);
    let mut drawn = DRAWN.lock().unwrap_or_else(|poison| poison.into_inner());
    Some(
        drawn
            .entry(letters.to_owned())
            .or_insert_with(|| Arc::new(drawing(letters)))
            .clone(),
    )
}

/// `letters` drawn, centred on their tone's surface, as a PNG.
fn drawing(letters: &str) -> Vec<u8> {
    let (ink, surface) = TONES[letters.bytes().map(usize::from).sum::<usize>() % TONES.len()];
    let scale = EM / UNITS;
    let glyphs: Vec<_> = letters.bytes().map(|b| &LETTERS[usize::from(b - b'A')]).collect();
    let width: f32 = glyphs.iter().map(|glyph| glyph.advance * scale).sum();
    let baseline = (SIZE as f32 + CAP_HEIGHT * scale) / 2.0;
    let mut x = (SIZE as f32 - width) / 2.0;
    let mut edges = Vec::new();
    for glyph in glyphs {
        outline(glyph.path, |(x0, y0), (x1, y1)| {
            edges.push(Edge {
                x0: x + x0 * scale,
                y0: baseline - y0 * scale,
                x1: x + x1 * scale,
                y1: baseline - y1 * scale,
            });
        });
        x += glyph.advance * scale;
    }
    let covered = coverage(&edges);
    let mut pixels = Vec::with_capacity(SIZE * SIZE * 3);
    for alpha in covered {
        for (&ink, &surface) in ink.iter().zip(&surface) {
            let blended = f32::from(surface) + (f32::from(ink) - f32::from(surface)) * alpha;
            pixels.push(blended.round() as u8);
        }
    }
    png(SIZE, SIZE, &pixels)
}

/// A straight piece of an outline, in pixels, the way it runs.
struct Edge {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

/// Each straight piece of the outline `path` describes, its curves made of eight each, handed to
/// `line` from and to, in font units.
fn outline(path: &str, mut line: impl FnMut((f32, f32), (f32, f32))) {
    let mut pen = Pen::default();
    let mut command = None;
    let (mut numbers, mut number) = (Vec::new(), String::new());
    for character in path.chars().chain([' ']) {
        if character.is_ascii_digit() || character == '.' || character == '-' {
            number.push(character);
            continue;
        }
        if !number.is_empty() {
            numbers.push(number.parse().unwrap_or(0.0));
            number.clear();
        }
        if character.is_ascii_alphabetic() {
            if let Some(command) = command {
                pen.draw(command, &numbers, &mut line);
            }
            numbers.clear();
            command = Some(character);
        }
    }
    if let Some(command) = command {
        pen.draw(command, &numbers, &mut line);
    }
}
/// Where an outline is being drawn from, and where its contour began.
#[derive(Default)]
struct Pen {
    at: (f32, f32),
    start: (f32, f32),
}
impl Pen {
    /// Draws one command of path data with its numbers.
    fn draw(&mut self, command: char, numbers: &[f32], line: &mut impl FnMut((f32, f32), (f32, f32))) {
        const STEPS: usize = 8;
        match (command, numbers) {
            ('M', &[x, y]) => {
                self.at = (x, y);
                self.start = self.at;
            }
            ('L', &[x, y]) => {
                line(self.at, (x, y));
                self.at = (x, y);
            }
            ('Q', &[cx, cy, x, y]) => {
                let (from, mut previous) = (self.at, self.at);
                for step in 1..=STEPS {
                    let t = step as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    let to = (
                        u * u * from.0 + 2.0 * u * t * cx + t * t * x,
                        u * u * from.1 + 2.0 * u * t * cy + t * t * y,
                    );
                    line(previous, to);
                    previous = to;
                }
                self.at = (x, y);
            }
            ('Z', _) => {
                line(self.at, self.start);
                self.at = self.start;
            }
            _ => {}
        }
    }
}

/// How much of each pixel, row by row, the outline `edges` make covers, by the nonzero rule:
/// sixteen lines through each row, each crossing a pixel's width as far as it's inside.
fn coverage(edges: &[Edge]) -> Vec<f32> {
    const LINES: usize = 16;
    let mut covered = vec![0.0f32; SIZE * SIZE];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..SIZE {
        let pixels = &mut covered[row * SIZE..(row + 1) * SIZE];
        for line in 0..LINES {
            let y = row as f32 + (line as f32 + 0.5) / LINES as f32;
            crossings.clear();
            for edge in edges {
                let (top, bottom, winding) = if edge.y0 < edge.y1 {
                    (edge.y0, edge.y1, 1)
                } else {
                    (edge.y1, edge.y0, -1)
                };
                if y < top || y >= bottom {
                    continue;
                }
                let x = edge.x0 + (y - edge.y0) * (edge.x1 - edge.x0) / (edge.y1 - edge.y0);
                crossings.push((x, winding));
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    fill(pixels, pair[0].0, pair[1].0, 1.0 / LINES as f32);
                }
            }
        }
    }
    for pixel in &mut covered {
        *pixel = pixel.clamp(0.0, 1.0);
    }
    covered
}
/// Adds `weight` to each pixel of `row` as far as the span from `from` to `to` crosses it.
fn fill(row: &mut [f32], from: f32, to: f32, weight: f32) {
    let (from, to) = (from.clamp(0.0, SIZE as f32), to.clamp(0.0, SIZE as f32));
    if to <= from {
        return;
    }
    let (first, last) = (from.floor() as usize, (to.ceil() as usize).min(SIZE));
    for (pixel, value) in row.iter_mut().enumerate().take(last).skip(first) {
        let crossed = (to.min(pixel as f32 + 1.0) - from.max(pixel as f32)).max(0.0);
        *value += crossed * weight;
    }
}

/// `pixels`, rows of red, green and blue bytes, as a PNG `width` by `height`.
fn png(width: usize, height: usize, pixels: &[u8]) -> Vec<u8> {
    let mut rows = Vec::with_capacity(height * (width * 3 + 1));
    for row in pixels.chunks(width * 3) {
        // Each row unfiltered.
        rows.push(0);
        rows.extend_from_slice(row);
    }
    let mut packed = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    packed.write_all(&rows).expect("writing to memory doesn't fail");
    let packed = packed.finish().expect("writing to memory doesn't fail");
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&(width as u32).to_be_bytes());
    header.extend_from_slice(&(height as u32).to_be_bytes());
    // Eight bits a sample, in colour, compressed and filtered as PNG does, not interlaced.
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(b"IHDR", &header), (b"IDAT", &packed), (b"IEND", &Vec::new())] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        png.extend_from_slice(&crc.sum().to_be_bytes());
    }
    png
}

#[cfg(test)]
#[path = "../../tests/backend/initials.rs"]
mod tests;
