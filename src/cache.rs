use std::env;
use std::path::PathBuf;

const APP_CACHE: &str = "krad-spotify";

#[must_use]
pub fn root() -> Option<PathBuf> {
    system_cache().map(|base| base.join(APP_CACHE))
}

#[cfg(target_os = "macos")]
fn system_cache() -> Option<PathBuf> {
    home().map(|home| home.join("Library").join("Caches"))
}

#[cfg(windows)]
fn system_cache() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn system_cache() -> Option<PathBuf> {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|home| home.join(".cache")))
}

#[cfg(not(windows))]
fn home() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}
