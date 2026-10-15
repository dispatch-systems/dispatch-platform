//! The photos it keeps: files in the DSP's cache, named by their digest and kind, written
//! before a collection publishes and removed once the roster no longer lists whoever they
//! show. Backups leave the cache out: a restore collects them again.
use dispatch_core::{
    Error, Result,
    db::{self, CACHE, Store},
};
use dispatch_cortex::associates::{Photo, PhotoFormat};
use std::path::{Path, PathBuf};

/// The folder the DSP's photos are in.
pub(crate) fn dir(store: &Store, dsp: &str) -> Result<PathBuf> {
    let owner = db::private_dir(&store.area(dsp, CACHE)?.join("driver_profiles"))?;
    db::private_dir(&owner.join("photos"))
}
/// The kinds a photo may be, by the extension its file takes.
const KINDS: [PhotoFormat; 5] = [
    PhotoFormat::Jpeg,
    PhotoFormat::Png,
    PhotoFormat::Webp,
    PhotoFormat::Gif,
    PhotoFormat::Avif,
];
/// The kind of photo an extension names.
pub(crate) fn kind(extension: &str) -> Option<PhotoFormat> {
    KINDS.into_iter().find(|k| k.extension() == extension)
}
/// The file a photo is kept in: its digest, then its extension. `None` for a digest or an
/// extension it could not have written.
pub(crate) fn file_name(sha256: &str, extension: &str) -> Option<String> {
    (sha256.len() == 64
        && sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && kind(extension).is_some())
    .then(|| format!("{sha256}.{extension}"))
}
/// Writes each photo's file, unless one of the same digest is there already, then keeps only
/// what publishing needs of it.
pub(crate) fn write(dir: &Path, photos: &mut [Photo]) -> Result<()> {
    for photo in photos {
        let name = file_name(&photo.sha256, photo.format.extension())
            .ok_or_else(|| Error::new("associates_capture_invalid", 502))?;
        let path = dir.join(name);
        if !path.is_file() {
            db::write_private(&path, &photo.bytes()?)?;
        }
        photo.data.clear();
    }
    Ok(())
}
/// Removes the files `names` the photos no longer need.
pub(crate) fn remove(dir: &Path, names: &[String]) -> Result<()> {
    for name in names {
        match std::fs::remove_file(dir.join(name)) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
