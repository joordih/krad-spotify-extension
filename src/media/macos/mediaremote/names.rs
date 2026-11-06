use super::super::process::{self, SCRIPT_TIMEOUT};
use std::collections::HashMap;

const BUNDLE_ID_SEPARATOR: char = '.';

#[derive(Default)]
pub(super) struct AppNames(HashMap<String, String>);

impl AppNames {
    pub(super) fn get(&mut self, id: &str) -> String {
        self.0
            .entry(id.to_owned())
            .or_insert_with(|| app_name(id).unwrap_or_else(|| fallback_name(id)))
            .clone()
    }
}

fn app_name(id: &str) -> Option<String> {
    if !process::is_bundle_id(id) {
        return None;
    }
    let stdout = process::osascript(
        &format!("return name of application id \"{id}\""),
        SCRIPT_TIMEOUT,
    )
    .ok()?;
    let name = String::from_utf8_lossy(&stdout).trim().to_owned();
    (!name.is_empty()).then_some(name)
}

fn fallback_name(id: &str) -> String {
    id.rsplit(BUNDLE_ID_SEPARATOR)
        .next()
        .unwrap_or(id)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_apps_are_called_after_their_bundle_id() {
        assert_eq!(fallback_name("com.google.Chrome"), "Chrome");
        assert_eq!(fallback_name("vlc"), "vlc");
    }
}
