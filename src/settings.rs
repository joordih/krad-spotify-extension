//! User settings declared in `extension.toml` (`[[contributes.setting]]`).

use crate::spotify;
use krad_extension_sdk::api::protocol::{SettingValue, SettingsValues};
use std::time::Duration;

// Keys of the settings declared in `extension.toml`.
const SETTING_POLL_INTERVAL: &str = "poll-interval";
const SETTING_VOLUME_STEP: &str = "volume-step";
const SETTING_SHOW_COVER: &str = "show-cover";
const MIN_POLL_SECS: f64 = 1.0;
const MAX_POLL_SECS: f64 = 30.0;
const MIN_VOLUME_STEP: f64 = 1.0;
const MAX_VOLUME_STEP: f64 = 50.0;

/// User configuration; the defaults are what the extension did before it had any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub poll_interval: Duration,
    pub volume_step: u8,
    pub show_cover: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            volume_step: spotify::DEFAULT_VOLUME_STEP,
            show_cover: true,
        }
    }
}

impl Settings {
    /// Reads the values the host sends. A missing, mistyped or non-finite value keeps its default
    /// and an out-of-range number is moved to the nearest limit, so a bad setting never breaks the
    /// view or the controls.
    pub fn from_values(values: &SettingsValues) -> Self {
        let mut settings = Self::default();
        for (key, value) in values {
            match (key.as_ref(), value) {
                (SETTING_POLL_INTERVAL, SettingValue::Number(secs)) if secs.is_finite() => {
                    settings.poll_interval =
                        Duration::from_secs_f64(secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS));
                }
                (SETTING_VOLUME_STEP, SettingValue::Number(step)) if step.is_finite() => {
                    settings.volume_step =
                        step.round().clamp(MIN_VOLUME_STEP, MAX_VOLUME_STEP) as u8;
                }
                (SETTING_SHOW_COVER, SettingValue::Toggle(show)) => settings.show_cover = *show,
                _ => {}
            }
        }
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::SettingKey;

    fn values(entries: &[(&str, SettingValue)]) -> SettingsValues {
        entries
            .iter()
            .map(|(key, value)| (key.parse::<SettingKey>().unwrap(), value.clone()))
            .collect()
    }

    #[test]
    fn no_settings_keep_the_defaults() {
        assert_eq!(
            Settings::from_values(&SettingsValues::new()),
            Settings::default()
        );
    }

    #[test]
    fn settings_are_read_from_the_host_values() {
        let settings = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Number(5.0)),
            (SETTING_VOLUME_STEP, SettingValue::Number(20.0)),
            (SETTING_SHOW_COVER, SettingValue::Toggle(false)),
        ]));
        assert_eq!(settings.poll_interval, Duration::from_secs(5));
        assert_eq!(settings.volume_step, 20);
        assert!(!settings.show_cover);
    }

    #[test]
    fn out_of_range_numbers_move_to_the_nearest_limit() {
        let low = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Number(0.0)),
            (SETTING_VOLUME_STEP, SettingValue::Number(-4.0)),
        ]));
        assert_eq!(low.poll_interval, Duration::from_secs(1));
        assert_eq!(low.volume_step, 1);
        let high = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Number(9999.0)),
            (SETTING_VOLUME_STEP, SettingValue::Number(500.0)),
        ]));
        assert_eq!(high.poll_interval, Duration::from_secs(30));
        assert_eq!(high.volume_step, 50);
    }

    #[test]
    fn mistyped_or_non_finite_values_keep_the_defaults() {
        let settings = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Text("fast".to_owned())),
            (SETTING_VOLUME_STEP, SettingValue::Number(f64::NAN)),
            (SETTING_SHOW_COVER, SettingValue::Number(0.0)),
            ("unknown", SettingValue::Toggle(false)),
        ]));
        assert_eq!(settings, Settings::default());
    }
}
