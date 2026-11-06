//! Caches Spotify artwork so the view can show it with
//! `ImageSource::Bundled`, the only image source Krad's renderer draws.
//!
//! Bundled images must live under the extension folder, but Krad fingerprints
//! that folder's content to decide whether the user trusts the extension, so
//! a new cover there would ask for trust again. `<folder>/cache` is therefore
//! a symbolic link (a junction on Windows) to a per-user cache directory:
//! Krad records only the link's target, which never changes.
//!
//! The renderer does not clip images, so rounded corners are baked into the
//! cached PNG as transparent pixels.
//!
//! The source image is either downloaded from an HTTPS URL with `curl`
//! (macOS, Linux) or read from the OS media session thumbnail (Windows).

use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::spotify::{self, Artwork};

const CACHE_DIR: &str = "cache";
/// Directory name inside the system cache directory.
const APP_CACHE: &str = "krad-spotify";
const PREFIX: &str = "cover-";
const MAX_NAME: usize = 64;
/// macOS ships curl at a fixed path; elsewhere it is looked up on `PATH`
/// (Windows 10+ includes `curl.exe`, most Linux distributions include curl).
#[cfg(target_os = "macos")]
const CURL: &str = "/usr/bin/curl";
#[cfg(not(target_os = "macos"))]
const CURL: &str = "curl";
/// Rendered size in the view, in logical pixels.
pub const COVER_SIZE: u16 = 160;
/// Pixels stored per logical pixel, so the cover stays sharp on Retina.
const SCALE: u32 = 2;
/// Corner radius in logical pixels.
const CORNER_RADIUS: f32 = 14.0;

/// Extension folder: the binary lives at `<folder>/bin/<binary>`.
fn extension_folder() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    Some(exe.parent()?.parent()?.to_path_buf())
}

/// The per-user cache directory for covers, outside the extension folder.
fn system_cache_dir() -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = home()?.join("Library").join("Caches");
    #[cfg(windows)]
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|home| home.join(".cache")))?;
    #[cfg(windows)]
    let _ = home;
    Some(base.join(APP_CACHE))
}

#[cfg(unix)]
fn make_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// A directory symlink needs Developer Mode or admin rights on Windows; a
/// junction does not.
#[cfg(windows)]
fn make_link(target: &Path, link: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    /// Hides the console window of the helper process.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if std::os::windows::fs::symlink_dir(target, link).is_ok() {
        return Ok(());
    }
    let status = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("mklink /J failed"))
    }
}

#[cfg(not(any(unix, windows)))]
fn make_link(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("links are not supported"))
}

/// The directory covers are written to, reached through `<folder>/cache`.
///
/// Makes `<folder>/cache` a link to the system cache directory, replacing an
/// older in-folder cache. When no link can be made it falls back to a plain
/// directory, which works but changes the folder's fingerprint.
fn cache_dir(folder: &Path) -> Option<PathBuf> {
    let link = folder.join(CACHE_DIR);
    let plain = |dir: PathBuf| fs::create_dir_all(&dir).ok().map(|()| dir);
    let Some(external) = system_cache_dir().filter(|dir| fs::create_dir_all(dir).is_ok()) else {
        return plain(link);
    };
    match fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if fs::read_link(&link).ok().as_deref() == Some(external.as_path()) {
                return Some(link);
            }
            // A link elsewhere (or a junction on Windows) is removed as a file or a directory.
            let _ = fs::remove_file(&link).or_else(|_| fs::remove_dir(&link));
        }
        // An in-folder cache from an older version only holds covers.
        Ok(meta) if meta.is_dir() => {
            let _ = fs::remove_dir_all(&link);
        }
        Ok(_) => {
            let _ = fs::remove_file(&link);
        }
        Err(_) => {}
    }
    if make_link(&external, &link).is_ok() {
        Some(link)
    } else {
        plain(link)
    }
}

fn file_name(source: &str) -> String {
    let id: String = source
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_NAME)
        .collect();
    if id.is_empty() {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        format!("{PREFIX}{COVER_SIZE}-{:016x}.png", hasher.finish())
    } else {
        format!("{PREFIX}{COVER_SIZE}-{id}.png")
    }
}

fn prune(dir: &Path, keep: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(PREFIX) && name != keep {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Coverage of the pixel at (`x`, `y`) by a square of `size` with rounded
/// corners of `radius`, antialiased over one pixel.
fn corner_alpha(x: u32, y: u32, size: u32, radius: f32) -> f32 {
    let center = |coord: u32| {
        let pos = coord as f32 + 0.5;
        let far = size as f32 - radius;
        if pos < radius {
            Some(radius)
        } else if pos > far {
            Some(far)
        } else {
            None
        }
    };
    match (center(x), center(y)) {
        (Some(cx), Some(cy)) => {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0)
        }
        _ => 1.0,
    }
}

fn rounded(bytes: &[u8]) -> Option<RgbaImage> {
    let size = u32::from(COVER_SIZE) * SCALE;
    let radius = CORNER_RADIUS * SCALE as f32;
    let mut cover = image::load_from_memory(bytes)
        .ok()?
        .resize_to_fill(size, size, FilterType::Lanczos3)
        .to_rgba8();
    for (x, y, Rgba(pixel)) in cover.enumerate_pixels_mut() {
        let alpha = corner_alpha(x, y, size, radius);
        if alpha < 1.0 {
            pixel[3] = (f32::from(pixel[3]) * alpha).round() as u8;
        }
    }
    Some(cover)
}

fn download(url: &str) -> Option<Vec<u8>> {
    let mut command = Command::new(CURL);
    command.args(["-fsSL", "--max-time", "5", "--proto", "=https", url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// Hides the console window of the helper process.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    (output.status.success() && !output.stdout.is_empty()).then_some(output.stdout)
}

/// Returns the rounded artwork path relative to the extension folder,
/// fetching it on first use. `None` when the URL is not HTTPS or fetching or
/// decoding fails.
pub fn local_cover(artwork: &Artwork) -> Option<PathBuf> {
    let source = match artwork {
        Artwork::Url(url) if url.starts_with("https://") => url,
        Artwork::Url(_) => return None,
        Artwork::Thumbnail(key) => key,
    };
    let dir = cache_dir(&extension_folder()?)?;
    let name = file_name(source);
    let target = dir.join(&name);
    if !target.is_file() {
        let bytes = match artwork {
            Artwork::Url(url) => download(url)?,
            Artwork::Thumbnail(_) => spotify::thumbnail()?,
        };
        let cover = rounded(&bytes)?;
        let partial = dir.join(format!("{name}.part"));
        if cover
            .save_with_format(&partial, image::ImageFormat::Png)
            .is_err()
            || fs::rename(&partial, &target).is_err()
        {
            let _ = fs::remove_file(&partial);
            return None;
        }
        prune(&dir, &name);
    }
    Some(Path::new(CACHE_DIR).join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn the_cache_is_a_link_out_of_the_extension_folder() {
        let folder = std::env::temp_dir().join(format!("krad-spotify-test-{}", std::process::id()));
        let old_cache = folder.join(CACHE_DIR);
        fs::create_dir_all(&old_cache).unwrap();
        fs::write(old_cache.join("cover-160-old.png"), b"x").unwrap();

        let dir = cache_dir(&folder).unwrap();
        assert_eq!(dir, folder.join(CACHE_DIR));
        let meta = fs::symlink_metadata(&dir).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "old cache not replaced by a link"
        );
        assert_eq!(fs::read_link(&dir).unwrap(), system_cache_dir().unwrap());
        // A second call keeps the link as it is.
        assert_eq!(cache_dir(&folder).unwrap(), dir);
        fs::remove_file(&dir).unwrap();
        fs::remove_dir(&folder).unwrap();
    }

    #[test]
    fn corners_are_transparent_and_center_is_opaque() {
        assert!(corner_alpha(0, 0, 128, 16.0) < f32::EPSILON);
        assert!(corner_alpha(127, 127, 128, 16.0) < f32::EPSILON);
        assert!((corner_alpha(64, 64, 128, 16.0) - 1.0).abs() < f32::EPSILON);
        assert!((corner_alpha(0, 64, 128, 16.0) - 1.0).abs() < f32::EPSILON);
    }

    /// Needs network: `cargo test -- --ignored`. Writes the result to the temp dir.
    #[test]
    #[ignore]
    fn rounds_a_real_cover() {
        let url = "https://i.scdn.co/image/ab67616d0000b27371f59b64b173166c29d0e91d";
        let cover = rounded(&download(url).unwrap()).unwrap();
        assert_eq!(cover.get_pixel(0, 0)[3], 0);
        cover
            .save(std::env::temp_dir().join("krad-cover-test.png"))
            .unwrap();
    }
}
