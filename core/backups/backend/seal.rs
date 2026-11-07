//! How a backup's files are kept where they're sent: each compressed, then sealed with the
//! backups' key in pieces of `PIECE` bytes, each piece its own AES-256-GCM message. Its nonce is
//! the file's own random start, the piece's place and whether it's the last, so no piece can be
//! moved, dropped or added unseen, and a file can't be cut short at a piece's end. What it's
//! sealed for (`binding`, its DSP) is in every piece's associated data, so one DSP's file can't
//! pass for another's.
//!
//! A sealed file is `MAGIC`, the 7 bytes that start each nonce, then each piece's ciphertext and
//! tag: `PIECE + 16` bytes, the last fewer.
use crate::{Error, Result, foundation::crypto};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
};

const MAGIC: &[u8; 8] = b"DSPBAK1\n";
/// How many bytes of the compressed file each piece seals.
const PIECE: usize = 1 << 20;
const TAG: usize = 16;
const START: usize = 7;

fn cipher(key: &[u8; 32]) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key).expect("a 32-byte key")
}
fn nonce(start: &[u8; START], place: u32, last: bool) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[..START].copy_from_slice(start);
    nonce[START..11].copy_from_slice(&place.to_be_bytes());
    nonce[11] = u8::from(last);
    nonce
}

/// Seals what's written to it into `out`, a piece at a time.
struct Sealer<W: Write> {
    out: W,
    cipher: Aes256Gcm,
    start: [u8; START],
    binding: Vec<u8>,
    piece: Vec<u8>,
    place: u32,
}
impl<W: Write> Sealer<W> {
    fn new(mut out: W, key: &[u8; 32], binding: &str) -> Result<Self> {
        let start = crypto::random::<START>()?;
        out.write_all(MAGIC)?;
        out.write_all(&start)?;
        Ok(Self {
            out,
            cipher: cipher(key),
            start,
            binding: binding.as_bytes().to_vec(),
            piece: Vec::with_capacity(PIECE),
            place: 0,
        })
    }
    fn seal(&mut self, last: bool) -> io::Result<()> {
        let nonce = nonce(&self.start, self.place, last);
        let sealed = self
            .cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &self.piece,
                    aad: &self.binding,
                },
            )
            .map_err(|_| io::Error::other("seal"))?;
        self.out.write_all(&sealed)?;
        self.piece.clear();
        self.place = self
            .place
            .checked_add(1)
            .ok_or_else(|| io::Error::other("too_large"))?;
        Ok(())
    }
    fn finish(mut self) -> io::Result<W> {
        self.seal(true)?;
        self.out.flush()?;
        Ok(self.out)
    }
}
impl<W: Write> Write for Sealer<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let room = PIECE - self.piece.len();
        let taken = bytes.len().min(room);
        self.piece.extend_from_slice(&bytes[..taken]);
        if self.piece.len() == PIECE {
            self.seal(false)?;
        }
        Ok(taken)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Seals the file `from` into `to`, compressed, for `binding`, and gives back the SHA-256 of what
/// it read and how many bytes that was.
pub fn seal(key: &[u8; 32], binding: &str, from: &Path, to: &Path) -> Result<(String, u64)> {
    let mut source = BufReader::new(File::open(from)?);
    let sealer = Sealer::new(BufWriter::new(File::create(to)?), key, binding)?;
    let mut compressed = GzEncoder::new(sealer, Compression::default());
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        size += read as u64;
        compressed.write_all(&buffer[..read])?;
    }
    compressed
        .finish()?
        .finish()?
        .into_inner()
        .map_err(|e| e.into_error())?;
    Ok((crypto::hex(&hash.finalize()), size))
}

/// Reads a sealed file back, a piece at a time, as it was before it was compressed.
struct Opener<R: Read> {
    input: R,
    cipher: Aes256Gcm,
    start: [u8; START],
    binding: Vec<u8>,
    /// The piece read ahead, sealed, to know whether the one before it was the last.
    ahead: Vec<u8>,
    opened: Vec<u8>,
    at: usize,
    place: u32,
    done: bool,
}
impl<R: Read> Opener<R> {
    fn new(mut input: R, key: &[u8; 32], binding: &str) -> Result<Self> {
        let mut head = [0; 8 + START];
        input
            .read_exact(&mut head)
            .map_err(|_| Error::new("invalid_backup", 400))?;
        crate::ensure(&head[..8] == MAGIC, "invalid_backup", 400)?;
        let mut opener = Self {
            input,
            cipher: cipher(key),
            start: head[8..].try_into().expect("seven bytes"),
            binding: binding.as_bytes().to_vec(),
            ahead: Vec::new(),
            opened: Vec::new(),
            at: 0,
            place: 0,
            done: false,
        };
        opener.ahead = opener.sealed_piece()?;
        Ok(opener)
    }
    fn sealed_piece(&mut self) -> io::Result<Vec<u8>> {
        let mut piece = Vec::with_capacity(PIECE + TAG);
        (&mut self.input)
            .take((PIECE + TAG) as u64)
            .read_to_end(&mut piece)?;
        Ok(piece)
    }
    fn open_next(&mut self) -> io::Result<()> {
        let piece = std::mem::take(&mut self.ahead);
        let last = piece.len() < PIECE + TAG || {
            self.ahead = self.sealed_piece()?;
            self.ahead.is_empty()
        };
        let nonce = nonce(&self.start, self.place, last);
        self.opened = self
            .cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &piece,
                    aad: &self.binding,
                },
            )
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid_backup"))?;
        self.at = 0;
        self.place += 1;
        self.done = last;
        Ok(())
    }
}
impl<R: Read> Read for Opener<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.at == self.opened.len() {
            if self.done {
                return Ok(0);
            }
            self.open_next()?;
        }
        let given = out.len().min(self.opened.len() - self.at);
        out[..given].copy_from_slice(&self.opened[self.at..self.at + given]);
        self.at += given;
        Ok(given)
    }
}

/// The sealed file `from`, opened for `binding` and written to `to` as it was sealed, checked
/// against the SHA-256 it was sealed with: `invalid_backup` for anything else.
pub fn open(
    key: &[u8; 32],
    binding: &str,
    from: &Path,
    to: &mut impl Write,
    sha256: &str,
) -> Result<u64> {
    let opener = Opener::new(BufReader::new(File::open(from)?), key, binding)?;
    let mut plain = GzDecoder::new(opener);
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = plain
            .read(&mut buffer)
            .map_err(|_| Error::new("invalid_backup", 400))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        size += read as u64;
        to.write_all(&buffer[..read])?;
    }
    crate::ensure(
        crypto::hex(&hash.finalize()) == sha256,
        "invalid_backup",
        400,
    )?;
    Ok(size)
}

/// Seals `value` as JSON for `binding`, as a backup's description is kept: in memory, small.
pub fn seal_json(
    key: &[u8; 32],
    binding: &str,
    value: &serde_json::Value,
    to: &Path,
) -> Result<String> {
    let from = to.with_extension("json");
    std::fs::write(&from, serde_json::to_vec(value)?)?;
    let (sha256, _) = seal(key, binding, &from, to)?;
    std::fs::remove_file(from)?;
    Ok(sha256)
}

/// The JSON sealed in `from` for `binding`.
pub fn open_json(key: &[u8; 32], binding: &str, from: &Path) -> Result<serde_json::Value> {
    let opener = Opener::new(BufReader::new(File::open(from)?), key, binding)?;
    let mut plain = Vec::new();
    GzDecoder::new(opener)
        .read_to_end(&mut plain)
        .map_err(|_| Error::new("invalid_backup", 400))?;
    serde_json::from_slice(&plain).map_err(|_| Error::new("invalid_backup", 400))
}

#[cfg(test)]
#[path = "../tests/backend/seal.rs"]
mod tests;
