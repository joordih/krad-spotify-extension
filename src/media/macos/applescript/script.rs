use super::super::process::ScriptFailure;
use super::fields::{self, Fields};
use super::App;
use crate::media::MediaError;
use std::time::Duration;

const FIELD_SEPARATOR: char = '\u{1f}';
const RECORD_SEPARATOR: char = '\u{1e}';
const LINE_ENDS: [char; 2] = ['\r', '\n'];
const STATUS_OK: &str = "ok";
const STATUS_ERROR: &str = "error";
const OUTPUT: &str = "out";
const LIST: &str = "f";
const JOINED: &str = "on joined(l, d)\n\
                      set AppleScript's text item delimiters to d\n\
                      set t to l as text\n\
                      set AppleScript's text item delimiters to \"\"\n\
                      return t\n\
                      end joined";

pub(super) fn listing(apps: &[App], app_timeout: Duration) -> String {
    let reads: String = apps.iter().map(|app| read(*app, app_timeout)).collect();
    format!(
        "set {OUTPUT} to {{}}\n{reads}return my joined({OUTPUT}, {})\n{JOINED}",
        character(RECORD_SEPARATOR)
    )
}

fn read(app: App, timeout: Duration) -> String {
    let id = app.bundle_id();
    let separator = character(FIELD_SEPARATOR);
    format!(
        "if application id \"{id}\" is running then\n\
         try\n\
         with timeout of {secs} seconds\n\
         tell application id \"{id}\"\n\
         set {LIST} to {empty}\n\
         {block}\
         end tell\n\
         end timeout\n\
         set end of {OUTPUT} to my joined({{\"{id}\", \"{STATUS_OK}\"}} & {LIST}, {separator})\n\
         on error message number n\n\
         set end of {OUTPUT} to my joined({{\"{id}\", \"{STATUS_ERROR}\", n as text, message}}, {separator})\n\
         end try\n\
         end if\n",
        secs = timeout.as_secs(),
        empty = fields::empty_list(),
        block = fields::read_block(app, LIST),
    )
}

fn character(separator: char) -> String {
    format!("character id {}", u32::from(separator))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Record<'a> {
    Answered(App, Fields<'a>),
    Failed(App, MediaError),
}

pub(super) fn records(output: &str) -> impl Iterator<Item = Record<'_>> {
    output
        .trim_end_matches(LINE_ENDS)
        .split(RECORD_SEPARATOR)
        .filter_map(Record::parse)
}

impl<'a> Record<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let values: Vec<&str> = text.split(FIELD_SEPARATOR).collect();
        let (id, rest) = values.split_first()?;
        let app = App::from_id(id)?;
        let unexpected = || {
            MediaError::Backend(format!(
                "{} returned track metadata in an unexpected format",
                app.name()
            ))
        };
        Some(match rest.split_first() {
            Some((&STATUS_OK, values)) => Fields::parse(values).map_or_else(
                || Self::Failed(app, unexpected()),
                |fields| Self::Answered(app, fields),
            ),
            Some((&STATUS_ERROR, [number, message @ ..])) => {
                let failure = ScriptFailure {
                    number: number.parse().ok(),
                    message: message.first().copied().unwrap_or_default().to_owned(),
                };
                Self::Failed(app, failure.for_app(app.name()))
            }
            _ => Self::Failed(app, unexpected()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(app: App, status: &str, values: &[&str]) -> String {
        let mut all = vec![app.bundle_id(), status];
        all.extend(values);
        all.join(&FIELD_SEPARATOR.to_string())
    }

    #[test]
    fn app_errors_carry_their_number() {
        let denied = record(App::Spotify, STATUS_ERROR, &["-1743", "Not authorized"]);
        let late = record(
            App::Music,
            STATUS_ERROR,
            &["-1712", "AppleEvent timed out."],
        );
        let other = record(App::Music, STATUS_ERROR, &["-10000", "Broken"]);
        let output = [denied, late, other].join(&RECORD_SEPARATOR.to_string());
        let parsed: Vec<Record<'_>> = records(&output).collect();
        assert_eq!(
            parsed[..2],
            [
                Record::Failed(
                    App::Spotify,
                    MediaError::AutomationDenied {
                        app: "Spotify".to_owned()
                    }
                ),
                Record::Failed(App::Music, MediaError::TimedOut),
            ]
        );
        assert!(matches!(
            &parsed[2],
            Record::Failed(App::Music, MediaError::Backend(detail))
                if detail.contains("Music") && detail.contains("Broken")
        ));
    }

    #[test]
    fn malformed_and_empty_output_is_handled() {
        let short = record(App::Spotify, STATUS_OK, &["playing"]);
        assert!(matches!(
            records(&short).collect::<Vec<_>>()[..],
            [Record::Failed(App::Spotify, MediaError::Backend(_))]
        ));
        assert_eq!(records("\n").count(), 0);
    }
}
