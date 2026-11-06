use crate::media::MediaError;
use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{mpsc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

pub(super) const SCRIPT_TIMEOUT: Duration = Duration::from_secs(3);
const DRAIN_GRACE: Duration = Duration::from_millis(200);
const EXIT_POLL: Duration = Duration::from_millis(5);
const SPAWNER_THREAD: &str = "spawner";
const SPAWNER_GONE: &str = "the process spawner is not running";
const OSASCRIPT: &str = "/usr/bin/osascript";
const OPEN: &str = "/usr/bin/open";
const SCRIPT_FLAG: &str = "-e";
const BUNDLE_FLAG: &str = "-b";
const AUTOMATION_DENIED: i32 = -1743;
const APPLE_EVENT_TIMED_OUT: i32 = -1712;
const ERROR_NUMBER_OPEN: char = '(';
const ERROR_NUMBER_CLOSE: char = ')';
const OPTION_PREFIX: char = '-';

struct SpawnRequest {
    command: Command,
    reply: mpsc::Sender<io::Result<Child>>,
}

/// # Errors
///
/// Fails when the program cannot be started.
pub(crate) fn spawn(command: Command) -> io::Result<Child> {
    let gone = || io::Error::other(SPAWNER_GONE);
    let (reply, answer) = mpsc::channel();
    spawner()
        .ok_or_else(gone)?
        .send(SpawnRequest { command, reply })
        .map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
}

fn spawner() -> Option<&'static mpsc::Sender<SpawnRequest>> {
    static SPAWNER: OnceLock<Option<mpsc::Sender<SpawnRequest>>> = OnceLock::new();
    SPAWNER
        .get_or_init(|| {
            let (sender, requests) = mpsc::channel::<SpawnRequest>();
            thread::Builder::new()
                .name(SPAWNER_THREAD.to_owned())
                .spawn(move || {
                    for SpawnRequest { mut command, reply } in requests {
                        let _ = reply.send(command.spawn());
                    }
                })
                .ok()
                .map(|_| sender)
        })
        .as_ref()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScriptFailure {
    pub(super) number: Option<i32>,
    pub(super) message: String,
}

impl ScriptFailure {
    fn from_stderr(stderr: &[u8]) -> Self {
        let message = String::from_utf8_lossy(stderr).trim().to_owned();
        let number = message
            .strip_suffix(ERROR_NUMBER_CLOSE)
            .and_then(|rest| rest.rsplit_once(ERROR_NUMBER_OPEN))
            .and_then(|(_, number)| number.parse().ok());
        Self { number, message }
    }

    pub(super) fn for_app(self, app: &str) -> MediaError {
        match self.number {
            Some(AUTOMATION_DENIED) => MediaError::AutomationDenied {
                app: app.to_owned(),
            },
            Some(APPLE_EVENT_TIMED_OUT) => MediaError::TimedOut,
            _ => MediaError::Backend(format!("{app} did not answer: {}", self.message)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ScriptError {
    Process(MediaError),
    Failed(ScriptFailure),
}

impl ScriptError {
    pub(super) fn for_app(self, app: &str) -> MediaError {
        match self {
            Self::Process(error) => error,
            Self::Failed(failure) => failure.for_app(app),
        }
    }
}

impl From<MediaError> for ScriptError {
    fn from(error: MediaError) -> Self {
        Self::Process(error)
    }
}

impl From<ScriptError> for MediaError {
    fn from(error: ScriptError) -> Self {
        match error {
            ScriptError::Process(error) => error,
            ScriptError::Failed(ScriptFailure {
                number: Some(APPLE_EVENT_TIMED_OUT),
                ..
            }) => Self::TimedOut,
            ScriptError::Failed(failure) => Self::Backend(failure.message),
        }
    }
}

pub(super) fn output_within(mut command: Command, timeout: Duration) -> Result<Output, MediaError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let started = Instant::now();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawn(command)
        .map_err(|error| MediaError::Backend(format!("Could not start {program}: {error}")))?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = wait_within(&mut child, timeout).ok_or(MediaError::TimedOut)?;
    let rest = timeout.saturating_sub(started.elapsed()).max(DRAIN_GRACE);
    let stdout = stdout
        .recv_timeout(rest)
        .map_err(|_| MediaError::TimedOut)?
        .map_err(|error| MediaError::Backend(format!("Could not read {program}: {error}")))?;
    let stderr = stderr
        .recv_timeout(DRAIN_GRACE)
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn wait_within(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(EXIT_POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = match pipe {
            Some(mut pipe) => pipe.read_to_end(&mut bytes).map(|_| bytes),
            None => Ok(bytes),
        };
        let _ = sender.send(read);
    });
    receiver
}

pub(super) fn osascript(script: &str, timeout: Duration) -> Result<Vec<u8>, ScriptError> {
    let mut command = Command::new(OSASCRIPT);
    command.args([SCRIPT_FLAG, script]);
    let output = output_within(command, timeout)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(ScriptError::Failed(ScriptFailure::from_stderr(
            &output.stderr,
        )))
    }
}

pub(super) fn open_app(bundle_id: &str) -> Result<(), MediaError> {
    open(&[BUNDLE_FLAG, bundle_id])
}

pub(super) fn open_url(url: &str) -> Result<(), MediaError> {
    open(&[url])
}

fn open(args: &[&str]) -> Result<(), MediaError> {
    let mut command = Command::new(OPEN);
    command.args(args);
    let output = output_within(command, SCRIPT_TIMEOUT)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failure(OPEN, &output))
    }
}

pub(super) fn failure(program: &str, output: &Output) -> MediaError {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    MediaError::Backend(if detail.is_empty() {
        format!("{program} failed with status {}", output.status)
    } else {
        format!("{program} failed: {detail}")
    })
}

pub(super) fn is_bundle_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with(OPTION_PREFIX)
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_programs_are_abandoned() {
        let started = Instant::now();
        let mut sleep = Command::new("/bin/sleep");
        sleep.arg("5");
        let result = output_within(sleep, Duration::from_millis(100));
        assert_eq!(result.map(drop), Err(MediaError::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn only_plain_bundle_ids_are_accepted() {
        assert!(is_bundle_id("com.google.Chrome"));
        assert!(is_bundle_id("org.videolan.vlc"));
        assert!(!is_bundle_id("com.x\" & do shell script \"y"));
        assert!(!is_bundle_id("-a"));
        assert!(!is_bundle_id(""));
    }

    #[test]
    fn osascript_errors_are_classified_by_number() {
        let denied = ScriptFailure::from_stderr(
            b"0:57: execution error: Not authorized to send Apple events to Music. (-1743)\n",
        );
        assert_eq!(
            denied.for_app("Music"),
            MediaError::AutomationDenied {
                app: "Music".to_owned()
            }
        );
        let late = ScriptFailure::from_stderr(
            b"execution error: Music got an error: AppleEvent timed out. (-1712)",
        );
        assert_eq!(late.for_app("Music"), MediaError::TimedOut);
        let other = ScriptFailure::from_stderr(b"syntax error");
        assert_eq!(other.number, None);
        assert!(
            matches!(other.for_app("Spotify"), MediaError::Backend(detail) if detail.contains("Spotify"))
        );
    }
}
