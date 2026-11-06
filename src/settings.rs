use crate::media::IgnoredApps;
use krad_extension_sdk::api::protocol::{SettingValue, SettingsValues};
use std::time::Duration;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MIN_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);
const DEFAULT_VOLUME_STEP: u8 = 10;
const MIN_VOLUME_STEP: u8 = 1;
const MAX_VOLUME_STEP: u8 = 50;
const DEFAULT_SHOW_COVER: bool = true;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    PollInterval,
    VolumeStep,
    ShowCover,
    IgnoredApps,
}

impl Key {
    const ALL: [Self; 4] = [
        Self::PollInterval,
        Self::VolumeStep,
        Self::ShowCover,
        Self::IgnoredApps,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::PollInterval => "poll-interval",
            Self::VolumeStep => "volume-step",
            Self::ShowCover => "show-cover",
            Self::IgnoredApps => "ignored-apps",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub poll_interval: Duration,
    pub volume_step: u8,
    pub show_cover: bool,
    pub ignored_apps: IgnoredApps,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            volume_step: DEFAULT_VOLUME_STEP,
            show_cover: DEFAULT_SHOW_COVER,
            ignored_apps: IgnoredApps::default(),
        }
    }
}

impl Settings {
    #[must_use]
    pub fn from_values(values: &SettingsValues) -> Self {
        let mut settings = Self::default();
        for (name, value) in values {
            if let Some(key) = Key::from_name(name.as_ref()) {
                settings.apply(key, value);
            }
        }
        settings
    }

    fn apply(&mut self, key: Key, value: &SettingValue) {
        match (key, value) {
            (Key::PollInterval, SettingValue::Number(secs)) if secs.is_finite() => {
                self.poll_interval = poll_interval(*secs);
            }
            (Key::VolumeStep, SettingValue::Number(step)) if step.is_finite() => {
                self.volume_step = volume_step(*step);
            }
            (Key::ShowCover, SettingValue::Toggle(show)) => self.show_cover = *show,
            (Key::IgnoredApps, SettingValue::Text(apps)) => {
                self.ignored_apps = IgnoredApps::parse(apps);
            }
            _ => {}
        }
    }
}

fn poll_interval(secs: f64) -> Duration {
    Duration::from_secs_f64(secs.clamp(
        MIN_POLL_INTERVAL.as_secs_f64(),
        MAX_POLL_INTERVAL.as_secs_f64(),
    ))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the step is finite, rounded and clamped to MIN_VOLUME_STEP..=MAX_VOLUME_STEP first"
)]
fn volume_step(step: f64) -> u8 {
    step.round()
        .clamp(f64::from(MIN_VOLUME_STEP), f64::from(MAX_VOLUME_STEP)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::SettingKey;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn values(
        entries: &[(&str, SettingValue)],
    ) -> Result<SettingsValues, Box<dyn std::error::Error>> {
        entries
            .iter()
            .map(|(name, value)| Ok((name.parse::<SettingKey>()?, value.clone())))
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
    fn settings_are_read_from_the_host_values() -> TestResult {
        let settings = Settings::from_values(&values(&[
            ("poll-interval", SettingValue::Number(5.0)),
            ("volume-step", SettingValue::Number(20.0)),
            ("show-cover", SettingValue::Toggle(false)),
            ("ignored-apps", SettingValue::Text("Chrome, vlc".to_owned())),
        ])?);
        assert_eq!(settings.poll_interval, Duration::from_secs(5));
        assert_eq!(settings.volume_step, 20);
        assert!(!settings.show_cover);
        assert_eq!(settings.ignored_apps, IgnoredApps::parse("chrome,VLC"));
        Ok(())
    }

    #[test]
    fn out_of_range_numbers_move_to_the_nearest_limit() -> TestResult {
        let low = Settings::from_values(&values(&[
            ("poll-interval", SettingValue::Number(0.0)),
            ("volume-step", SettingValue::Number(-4.0)),
        ])?);
        assert_eq!(low.poll_interval, Duration::from_secs(1));
        assert_eq!(low.volume_step, 1);
        let high = Settings::from_values(&values(&[
            ("poll-interval", SettingValue::Number(9999.0)),
            ("volume-step", SettingValue::Number(500.0)),
        ])?);
        assert_eq!(high.poll_interval, Duration::from_secs(30));
        assert_eq!(high.volume_step, 50);
        Ok(())
    }

    #[test]
    fn mistyped_or_non_finite_values_keep_the_defaults() -> TestResult {
        let settings = Settings::from_values(&values(&[
            ("poll-interval", SettingValue::Text("fast".to_owned())),
            ("volume-step", SettingValue::Number(f64::NAN)),
            ("show-cover", SettingValue::Number(0.0)),
            ("ignored-apps", SettingValue::Toggle(true)),
            ("unknown", SettingValue::Toggle(false)),
        ])?);
        assert_eq!(settings, Settings::default());
        Ok(())
    }
}
