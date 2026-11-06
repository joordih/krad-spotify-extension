mod cache_dir;
mod download;
mod rounding;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::media::{content_key, Artwork};
use cache_dir::cache_dir;
use download::download;
use rounding::rounded;

pub const COVER_SIZE: u16 = 160;
const CACHE_DIR: &str = "cache";
const COVER_PREFIX: &str = "cover-";
const COVER_EXTENSION: &str = "png";
const PARTIAL_EXTENSION: &str = "part";
const URL_PATH_SEPARATOR: char = '/';
const MAX_NAME_CHARS: usize = 64;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

static EXTENSION_FOLDER: OnceLock<PathBuf> = OnceLock::new();

pub fn set_extension_folder(folder: PathBuf) {
    let _ = EXTENSION_FOLDER.set(folder);
}

fn extension_folder() -> Option<PathBuf> {
    if let Some(folder) = EXTENSION_FOLDER.get().filter(|folder| folder.is_dir()) {
        return Some(folder.clone());
    }
    bundled_binary_folder()
}

fn bundled_binary_folder() -> Option<PathBuf> {
    let binary = std::env::current_exe().ok()?.canonicalize().ok()?;
    let bin_dir = binary.parent()?;
    Some(bin_dir.parent()?.to_path_buf())
}

#[cfg(windows)]
fn without_console(command: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW)
}

fn file_name(source: &str) -> String {
    let id = readable_id(source);
    let id = if id.is_empty() {
        content_key(source)
    } else {
        id
    };
    format!("{COVER_PREFIX}{COVER_SIZE}-{id}.{COVER_EXTENSION}")
}

fn readable_id(source: &str) -> String {
    source
        .rsplit(URL_PATH_SEPARATOR)
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_NAME_CHARS)
        .collect()
}

fn cache_key(artwork: &Artwork) -> Option<String> {
    Some(match artwork {
        Artwork::Url(url) => url.as_str().to_owned(),
        Artwork::File(path) => {
            let meta = fs::metadata(path).ok().filter(fs::Metadata::is_file)?;
            content_key((path, meta.len(), meta.modified().ok()))
        }
        Artwork::Bytes(bytes) => bytes.key.clone(),
    })
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    fs::read(path).ok().filter(|bytes| !bytes.is_empty())
}

fn source_bytes(artwork: &Artwork) -> Option<Vec<u8>> {
    match artwork {
        Artwork::Url(url) => download(url.as_str()),
        Artwork::File(path) => read_file(path),
        Artwork::Bytes(bytes) => Some(bytes.data.to_vec()),
    }
}

fn prune(dir: &Path, keep: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(COVER_PREFIX) && name != keep {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn write_cover(artwork: &Artwork, dir: &Path, name: &str) -> Option<()> {
    let cover = rounded(&source_bytes(artwork)?)?;
    let target = dir.join(name);
    let partial = dir.join(format!("{name}.{PARTIAL_EXTENSION}"));
    let saved = cover
        .save_with_format(&partial, image::ImageFormat::Png)
        .is_ok();
    if !saved || fs::rename(&partial, &target).is_err() {
        let _ = fs::remove_file(&partial);
        return None;
    }
    prune(dir, name);
    Some(())
}

#[must_use]
pub fn local_cover(artwork: &Artwork) -> Option<PathBuf> {
    let key = cache_key(artwork)?;
    let dir = cache_dir(&extension_folder()?)?;
    let name = file_name(&key);
    if !dir.join(&name).is_file() {
        write_cover(artwork, &dir, &name)?;
    }
    Some(Path::new(CACHE_DIR).join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_artwork_kind_gets_a_stable_cache_name() -> Result<(), Box<dyn std::error::Error>> {
        let url = Artwork::from_url("https://i.scdn.co/image/ab67").ok_or("not artwork")?;
        let key = cache_key(&url).ok_or("no key")?;
        assert_eq!(file_name(&key), "cover-160-ab67.png");
        let bytes = Artwork::from_bytes(vec![1, 2, 3]).ok_or("no bytes")?;
        assert_eq!(cache_key(&bytes), cache_key(&bytes.clone()));
        assert_eq!(
            cache_key(&Artwork::File(PathBuf::from("/nonexistent/x.png"))),
            None
        );

        let file = std::env::temp_dir().join(format!("krad-art-test-{}", std::process::id()));
        fs::write(&file, b"image")?;
        let artwork = Artwork::File(file.clone());
        assert!(cache_key(&artwork).is_some());
        assert_eq!(read_file(&file).as_deref(), Some(&b"image"[..]));
        fs::remove_file(&file)?;
        Ok(())
    }
}
