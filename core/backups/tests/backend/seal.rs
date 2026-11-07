//! A sealed file opens as it was with its key and for what it was sealed for alone, and not once
//! any piece of it is changed, dropped, moved or cut short.
use super::*;

const KEY: [u8; 32] = [7; 32];
const DSP: &str = "dsp_0123456789abcdef0123456789abcdef";

/// Bytes that don't compress, so they seal into several pieces.
fn noise(length: usize) -> Vec<u8> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn a_sealed_file_opens_only_whole_and_for_its_own() {
    let folder = tempfile::tempdir().unwrap();
    let (plain, sealed) = (folder.path().join("plain"), folder.path().join("sealed"));
    let bytes = noise(PIECE * 2 + 1000);
    std::fs::write(&plain, &bytes).unwrap();
    let (sha256, size) = seal(&KEY, DSP, &plain, &sealed).unwrap();
    assert_eq!(
        (sha256.as_str(), size),
        (crypto::sha(&bytes).as_str(), bytes.len() as u64)
    );
    let mut opened = Vec::new();
    open(&KEY, DSP, &sealed, &mut opened, &sha256).unwrap();
    assert_eq!(opened, bytes);

    let whole = std::fs::read(&sealed).unwrap();
    let head = MAGIC.len() + START;
    let piece = PIECE + TAG;
    let mut flipped = whole.clone();
    flipped[head + piece + 5] ^= 1;
    let mut moved = whole[..head].to_vec();
    moved.extend_from_slice(&whole[head + piece..head + 2 * piece]);
    moved.extend_from_slice(&whole[head..head + piece]);
    moved.extend_from_slice(&whole[head + 2 * piece..]);
    for changed in [
        flipped,
        moved,
        // Cut short at a piece's end, as if the rest never came.
        whole[..head + 2 * piece].to_vec(),
        whole[..head + piece].to_vec(),
    ] {
        std::fs::write(&sealed, &changed).unwrap();
        let refused = open(&KEY, DSP, &sealed, &mut std::io::sink(), &sha256).unwrap_err();
        assert_eq!(refused.code, "invalid_backup");
    }
    std::fs::write(&sealed, &whole).unwrap();
    let other = "dsp_fedcba9876543210fedcba9876543210";
    for (key, binding) in [([8; 32], DSP), (KEY, other)] {
        let refused = open(&key, binding, &sealed, &mut std::io::sink(), &sha256).unwrap_err();
        assert_eq!(refused.code, "invalid_backup");
    }
}
