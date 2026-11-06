use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::CACHE_DIR;
use crate::cache;

#[cfg(windows)]
const SHELL: &str = "cmd";
#[cfg(windows)]
const MAKE_JUNCTION: [&str; 3] = ["/C", "mklink", "/J"];

enum Entry {
    Missing,
    Link(Option<PathBuf>),
    Directory,
    File,
}

impl Entry {
    fn at(path: &Path) -> Self {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return Self::Missing;
        };
        if meta.file_type().is_symlink() {
            return Self::Link(fs::read_link(path).ok());
        }
        if meta.is_dir() {
            return Self::Directory;
        }
        Self::File
    }

    fn remove(&self, path: &Path) {
        let _ = match self {
            Self::Missing => Ok(()),
            Self::Link(_) => fs::remove_file(path).or_else(|_| fs::remove_dir(path)),
            Self::Directory => fs::remove_dir_all(path),
            Self::File => fs::remove_file(path),
        };
    }
}

pub(super) fn cache_dir(folder: &Path) -> Option<PathBuf> {
    let link = folder.join(CACHE_DIR);
    let Some(external) = external_cache() else {
        return plain_dir(link);
    };
    let entry = Entry::at(&link);
    if let Entry::Link(Some(target)) = &entry {
        if *target == external {
            return Some(link);
        }
    }
    entry.remove(&link);
    if make_link(&external, &link).is_ok() {
        return Some(link);
    }
    plain_dir(link)
}

fn external_cache() -> Option<PathBuf> {
    cache::root().filter(|dir| fs::create_dir_all(dir).is_ok())
}

fn plain_dir(dir: PathBuf) -> Option<PathBuf> {
    fs::create_dir_all(&dir).ok().map(|()| dir)
}

#[cfg(unix)]
fn make_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_link(target: &Path, link: &Path) -> io::Result<()> {
    if std::os::windows::fs::symlink_dir(target, link).is_ok() {
        return Ok(());
    }
    let mut command = std::process::Command::new(SHELL);
    command.args(MAKE_JUNCTION).arg(link).arg(target);
    let status = super::without_console(&mut command).status()?;
    if !status.success() {
        return Err(io::Error::other("mklink /J failed"));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn make_link(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::other("links are not supported"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_cache_is_a_link_out_of_the_extension_folder() -> Result<(), Box<dyn std::error::Error>> {
        let folder = std::env::temp_dir().join(format!("krad-spotify-test-{}", std::process::id()));
        let old_cache = folder.join(CACHE_DIR);
        fs::create_dir_all(&old_cache)?;
        fs::write(old_cache.join("cover-160-old.png"), b"x")?;

        let dir = cache_dir(&folder).ok_or("no cache dir")?;
        assert_eq!(dir, folder.join(CACHE_DIR));
        assert!(
            fs::symlink_metadata(&dir)?.file_type().is_symlink(),
            "old cache not replaced by a link"
        );
        assert_eq!(fs::read_link(&dir).ok(), cache::root());
        assert_eq!(cache_dir(&folder), Some(dir.clone()));
        fs::remove_file(&dir)?;
        fs::remove_dir(&folder)?;
        Ok(())
    }
}
