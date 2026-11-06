use crate::cache;
use crate::media::MediaError;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

const SCRIPT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/vendor/mediaremote-adapter/bin/mediaremote-adapter.pl"
));
const DYLIB: &[u8] = include_bytes!(env!("MEDIAREMOTE_ADAPTER_DYLIB"));
const KEY: &str = env!("MEDIAREMOTE_ADAPTER_KEY");
const FOLDER_PREFIX: &str = "mediaremote-adapter-";
const SCRIPT_NAME: &str = "mediaremote-adapter.pl";
const FRAMEWORK_NAME: &str = "MediaRemoteAdapter.framework";
const DYLIB_NAME: &str = "MediaRemoteAdapter";
const PARTIAL_EXTENSION: &str = "partial";
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
const STALE_VERSION_DAYS: u64 = 7;
const STALE_VERSION_AGE: Duration = Duration::from_secs(STALE_VERSION_DAYS * SECONDS_PER_DAY);

#[derive(Debug)]
pub(super) struct Files {
    pub(super) script: PathBuf,
    pub(super) framework: PathBuf,
}

impl Files {
    fn in_folder(root: &Path, key: &str) -> Self {
        let folder = root.join(format!("{FOLDER_PREFIX}{key}"));
        Self {
            script: folder.join(SCRIPT_NAME),
            framework: folder.join(FRAMEWORK_NAME),
        }
    }

    fn dylib(&self) -> PathBuf {
        self.framework.join(DYLIB_NAME)
    }

    pub(super) fn ensure(&self) -> Result<(), MediaError> {
        self.restore(SCRIPT, DYLIB)
    }

    fn restore(&self, script: &[u8], dylib: &[u8]) -> Result<(), MediaError> {
        write_if_missing(&self.script, script)?;
        write_if_missing(&self.dylib(), dylib)
    }
}

pub(super) fn extract() -> Result<Files, MediaError> {
    let root = cache::root().ok_or(MediaError::Unavailable)?;
    let files = extract_into(&root, KEY, SCRIPT, DYLIB)?;
    remove_stale_versions(&root, KEY);
    Ok(files)
}

fn extract_into(root: &Path, key: &str, script: &[u8], dylib: &[u8]) -> Result<Files, MediaError> {
    let files = Files::in_folder(root, key);
    write_unless_identical(&files.script, script)?;
    write_unless_identical(&files.dylib(), dylib)?;
    Ok(files)
}

fn remove_stale_versions(root: &Path, key: &str) {
    let current = format!("{FOLDER_PREFIX}{key}");
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(FOLDER_PREFIX) && name != current && is_stale(&entry.path()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn is_stale(folder: &Path) -> bool {
    fs::metadata(folder)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_VERSION_AGE)
}

fn write_if_missing(path: &Path, bytes: &[u8]) -> Result<(), MediaError> {
    if path.is_file() {
        return Ok(());
    }
    write(path, bytes)
}

fn write_unless_identical(path: &Path, bytes: &[u8]) -> Result<(), MediaError> {
    if fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    write(path, bytes)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), MediaError> {
    write_atomically(path, bytes).map_err(|error| {
        MediaError::Backend(format!(
            "Could not write the MediaRemote adapter to {}: {error}",
            path.display()
        ))
    })
}

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let folder = path
        .parent()
        .ok_or_else(|| io::Error::other("no parent folder"))?;
    fs::create_dir_all(folder)?;
    let partial = path.with_extension(format!(
        "{PARTIAL_EXTENSION}-{}-{}",
        process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let written = fs::File::create(&partial)
        .and_then(|mut file| file.write_all(bytes))
        .and_then(|()| fs::rename(&partial, path));
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("krad-adapter-{name}-{}", process::id()))
    }

    fn folders_in(root: &Path) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = fs::read_dir(root)?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?;
        names.sort();
        Ok(names)
    }

    fn age(folder: &Path, by: Duration) -> io::Result<()> {
        let modified = SystemTime::now()
            .checked_sub(by)
            .ok_or_else(|| io::Error::other("time before the epoch"))?;
        fs::File::open(folder)?.set_modified(modified)
    }

    #[test]
    fn a_changed_adapter_of_the_same_size_is_rewritten() -> TestResult {
        let root = temp_root("rewrite");
        let files = extract_into(&root, "k", b"old script", b"dylib")?;
        fs::write(&files.script, b"new script")?;
        let files = extract_into(&root, "k", b"old script", b"dylib")?;
        assert_eq!(fs::read(&files.script)?, b"old script");
        assert_eq!(fs::read(files.dylib())?, b"dylib");
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn a_deleted_adapter_is_restored() -> TestResult {
        let root = temp_root("restore");
        let files = extract_into(&root, "k", b"script", b"dylib")?;
        fs::remove_dir_all(&root)?;
        files.restore(b"script", b"dylib")?;
        assert_eq!(fs::read(&files.script)?, b"script");
        assert_eq!(fs::read(files.dylib())?, b"dylib");
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn only_stale_other_adapter_versions_are_removed() -> TestResult {
        let root = temp_root("versions");
        let stale = extract_into(&root, "stale", b"script", b"dylib")?;
        extract_into(&root, "running", b"script", b"dylib")?;
        let current = extract_into(&root, "current", b"script", b"dylib")?;
        fs::create_dir_all(root.join("covers"))?;
        let long_ago = STALE_VERSION_AGE + Duration::from_secs(SECONDS_PER_DAY);
        age(stale.script.parent().ok_or("no adapter folder")?, long_ago)?;
        age(
            current.script.parent().ok_or("no adapter folder")?,
            long_ago,
        )?;
        remove_stale_versions(&root, "current");
        assert_eq!(
            folders_in(&root)?,
            [
                "covers",
                "mediaremote-adapter-current",
                "mediaremote-adapter-running"
            ]
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
