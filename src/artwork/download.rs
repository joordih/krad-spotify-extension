use std::process::{Command, Output};
use std::time::Duration;

#[cfg(target_os = "macos")]
const CURL: &str = "/usr/bin/curl";
#[cfg(not(target_os = "macos"))]
const CURL: &str = "curl";
const QUIET_FAIL_FOLLOW_REDIRECTS: &str = "-fsSL";
const MAX_TIME: &str = "--max-time";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(5);
const ALLOWED_PROTOCOLS: [&str; 2] = ["--proto", "=https"];

pub(super) fn download(url: &str) -> Option<Vec<u8>> {
    let mut command = Command::new(CURL);
    command
        .arg(QUIET_FAIL_FOLLOW_REDIRECTS)
        .arg(MAX_TIME)
        .arg(DOWNLOAD_TIMEOUT.as_secs().to_string())
        .args(ALLOWED_PROTOCOLS)
        .arg(url);
    #[cfg(windows)]
    super::without_console(&mut command);
    let output = run(command).ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(output.stdout)
}

#[cfg(target_os = "macos")]
fn run(mut command: Command) -> std::io::Result<Output> {
    use std::process::Stdio;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::media::spawn(command)?.wait_with_output()
}

#[cfg(not(target_os = "macos"))]
fn run(mut command: Command) -> std::io::Result<Output> {
    command.output()
}
