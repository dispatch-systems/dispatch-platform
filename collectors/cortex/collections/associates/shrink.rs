//! A photo as small as any page needs it: at most `SIDE` pixels on its long side, as a JPEG.
//! Amazon serves camera photos of several megapixels for faces no page draws larger than 96 px.
//! A JPEG is decoded straight at a half, a quarter or an eighth of its size, as its format
//! allows, so a 3,392 by 2,544 photo is read as 424 by 318 rather than whole; a PNG is read whole.
//! The JPEG written carries none of the original's metadata, so it is turned upright as the
//! original's EXIF says first, and the rest, a camera's GPS among it, goes.
use super::PhotoFormat;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use jpeg_decoder::{CodingProcess, PixelFormat};
use std::io::Cursor;

/// The long side of a photo kept, in pixels: four times the largest face a page draws.
pub const SIDE: u32 = 400;
/// The most a JPEG or PNG kept weighs, in bytes. A shrunk one weighs about 20 KB; one that
/// weighs more was kept before photos were shrunk.
pub const MAX_KEPT_PHOTO_BYTES: usize = 200 * 1024;
/// How much of the picture the JPEG written keeps, out of 100.
const QUALITY: u8 = 80;
/// The most pixels a JPEG read in rows at a fraction of its size may say it has before it is
/// read, past any camera's.
const MAX_PIXELS: u64 = 50_000_000;
/// The most pixels a photo read whole may say it has: a progressive JPEG, whose coefficients
/// are kept at full size, six bytes a pixel, or a PNG, up to four. With four photos shrunk at
/// once, a file claiming more can't make reading them take gigabytes.
const MAX_WHOLE_PIXELS: u64 = 16_000_000;
/// Why a photo was not kept when it could not be read, as the collection records it.
const UNREADABLE: &str = "photo_unreadable";

/// The photo as it is kept: `bytes` themselves when already small, else shrunk to a JPEG. A GIF,
/// WebP or AVIF is kept as it is: nothing here reads them. Refused, by a fixed label, when it is
/// no image, one that can't be read, or one claiming too many pixels.
pub fn shrink(bytes: Vec<u8>) -> Result<Vec<u8>, &'static str> {
    let picture = match PhotoFormat::of(&bytes) {
        Some(PhotoFormat::Jpeg) => jpeg(&bytes)?,
        Some(PhotoFormat::Png) => png(&bytes)?,
        Some(_) => return Ok(bytes),
        None => return Err("photo_not_an_image"),
    };
    let Some(picture) = picture else {
        return Ok(bytes);
    };
    picture.resized()?.upright().jpeg()
}

/// Pixels read from a photo: one channel for grey, three for colour, row by row.
struct Picture {
    width: u32,
    height: u32,
    channels: usize,
    pixels: Vec<u8>,
    /// How its EXIF says to turn it upright, 1 to 8; 1 when it says nothing.
    orientation: u16,
}

/// Whether a photo of these dimensions and this weight is kept as it is.
fn small(width: u32, height: u32, weight: usize) -> bool {
    width.max(height) <= SIDE && weight <= MAX_KEPT_PHOTO_BYTES
}
/// Whether a photo claims no more than `most` pixels.
fn readable(width: u32, height: u32, most: u64) -> Result<(), &'static str> {
    if u64::from(width) * u64::from(height) > most {
        return Err("photo_too_large");
    }
    Ok(())
}

/// A JPEG's pixels, read at the smallest fraction of its size that still covers `SIDE`; none
/// when it is small already.
fn jpeg(bytes: &[u8]) -> Result<Option<Picture>, &'static str> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    decoder.read_info().map_err(|_| UNREADABLE)?;
    let info = decoder.info().ok_or(UNREADABLE)?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let most = match info.coding_process {
        CodingProcess::DctSequential => MAX_PIXELS,
        CodingProcess::DctProgressive => MAX_WHOLE_PIXELS,
        // No camera writes one, and it can't be read at a fraction of its size.
        CodingProcess::Lossless => return Err(UNREADABLE),
    };
    readable(width, height, most)?;
    if small(width, height, bytes.len()) {
        return Ok(None);
    }
    let side = SIDE as u16;
    let (width, height) = decoder.scale(side, side).map_err(|_| UNREADABLE)?;
    let pixels = decoder.decode().map_err(|_| UNREADABLE)?;
    let (channels, pixels) = match info.pixel_format {
        PixelFormat::L8 => (1, pixels),
        // Only a lossless JPEG has it, refused above; a twelve-bit one fails to read before.
        PixelFormat::L16 => return Err(UNREADABLE),
        PixelFormat::RGB24 => (3, pixels),
        PixelFormat::CMYK32 => (
            3,
            pixels
                .chunks_exact(4)
                .flat_map(|cmyk| {
                    let ink = |channel: u8| {
                        let (paper, black) = (255 - u32::from(channel), 255 - u32::from(cmyk[3]));
                        (paper * black / 255) as u8
                    };
                    [ink(cmyk[0]), ink(cmyk[1]), ink(cmyk[2])]
                })
                .collect(),
        ),
    };
    Ok(Some(Picture {
        width: u32::from(width),
        height: u32::from(height),
        channels,
        pixels,
        orientation: decoder.exif_data().map_or(1, orientation),
    }))
}

/// A PNG's pixels, read whole, on white where it is see-through; none when it is small already.
fn png(bytes: &[u8]) -> Result<Option<Picture>, &'static str> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| UNREADABLE)?;
    let (width, height) = (reader.info().width, reader.info().height);
    readable(width, height, MAX_WHOLE_PIXELS)?;
    if small(width, height, bytes.len()) {
        return Ok(None);
    }
    let mut pixels = vec![0; reader.output_buffer_size().ok_or(UNREADABLE)?];
    let frame = reader.next_frame(&mut pixels).map_err(|_| UNREADABLE)?;
    pixels.truncate(frame.buffer_size());
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => on_white(&mut pixels, 1),
        png::ColorType::Rgba => on_white(&mut pixels, 3),
        // Expanded to colour above.
        png::ColorType::Indexed => return Err(UNREADABLE),
    };
    Ok(Some(Picture {
        width,
        height,
        channels,
        pixels,
        orientation: 1,
    }))
}

/// Pixels of `colours` channels and an alpha, laid on white where they are see-through, in place:
/// each pixel is written no later than it was read, and the alphas go. Gives `colours`.
fn on_white(pixels: &mut Vec<u8>, colours: usize) -> usize {
    let mut to = 0;
    for from in (0..pixels.len()).step_by(colours + 1) {
        let alpha = u32::from(pixels[from + colours]);
        for channel in 0..colours {
            let value = u32::from(pixels[from + channel]);
            pixels[to + channel] = ((value * alpha + 255 * (255 - alpha)) / 255) as u8;
        }
        to += colours;
    }
    pixels.truncate(to);
    colours
}

/// The orientation an EXIF block, from its TIFF header, gives: the first directory's tag 0x0112.
/// 1, upright, when it gives none or makes no sense.
fn orientation(exif: &[u8]) -> u16 {
    let little = match exif.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return 1,
    };
    let read16 = |at: usize| {
        exif.get(at..at + 2).map(|b| {
            let b = [b[0], b[1]];
            if little {
                u16::from_le_bytes(b)
            } else {
                u16::from_be_bytes(b)
            }
        })
    };
    let read32 = |at: usize| {
        exif.get(at..at + 4).map(|b| {
            let b = [b[0], b[1], b[2], b[3]];
            if little {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }
        })
    };
    let found = (|| {
        let directory = read32(4)? as usize;
        let entries = read16(directory)? as usize;
        (0..entries.min(256))
            .map(|entry| directory + 2 + entry * 12)
            .find(|&at| read16(at) == Some(0x0112))
            .and_then(|at| read16(at + 8))
    })();
    found.filter(|value| (1..=8).contains(value)).unwrap_or(1)
}

impl Picture {
    /// At most `SIDE` on its long side, its shape kept.
    fn resized(self) -> Result<Self, &'static str> {
        let long = self.width.max(self.height);
        if long <= SIDE {
            return Ok(self);
        }
        let fit = |side: u32| (u64::from(side) * u64::from(SIDE) / u64::from(long)).max(1) as u32;
        let (width, height) = (fit(self.width), fit(self.height));
        let pixel = if self.channels == 1 {
            PixelType::U8
        } else {
            PixelType::U8x3
        };
        let source = Image::from_vec_u8(self.width, self.height, self.pixels, pixel)
            .map_err(|_| UNREADABLE)?;
        let mut resized = Image::new(width, height, pixel);
        Resizer::new()
            .resize(
                &source,
                &mut resized,
                &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)),
            )
            .map_err(|_| UNREADABLE)?;
        Ok(Self {
            width,
            height,
            pixels: resized.into_vec(),
            ..self
        })
    }

    /// Turned and flipped as its EXIF orientation says, so it stands upright with none.
    fn upright(self) -> Self {
        if self.orientation == 1 {
            return self;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        // Orientations 5 to 8 turn it on its side.
        let (width, height) = if self.orientation >= 5 {
            (h, w)
        } else {
            (w, h)
        };
        // For each pixel of the upright picture, the pixel of the original it shows.
        let source = |x: usize, y: usize| match self.orientation {
            2 => (w - 1 - x, y),
            3 => (w - 1 - x, h - 1 - y),
            4 => (x, h - 1 - y),
            5 => (y, x),
            6 => (y, h - 1 - x),
            7 => (w - 1 - y, h - 1 - x),
            8 => (w - 1 - y, x),
            _ => (x, y),
        };
        let channels = self.channels;
        let mut pixels = Vec::with_capacity(self.pixels.len());
        for y in 0..height {
            for x in 0..width {
                let (sx, sy) = source(x, y);
                let at = (sy * w + sx) * channels;
                pixels.extend_from_slice(&self.pixels[at..at + channels]);
            }
        }
        Self {
            width: width as u32,
            height: height as u32,
            pixels,
            orientation: 1,
            ..self
        }
    }

    /// As a JPEG, with no metadata.
    fn jpeg(self) -> Result<Vec<u8>, &'static str> {
        let color = if self.channels == 1 {
            jpeg_encoder::ColorType::Luma
        } else {
            jpeg_encoder::ColorType::Rgb
        };
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, QUALITY)
            .encode(&self.pixels, self.width as u16, self.height as u16, color)
            .map_err(|_| UNREADABLE)?;
        Ok(out)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/associates/shrink.rs"]
mod tests;
