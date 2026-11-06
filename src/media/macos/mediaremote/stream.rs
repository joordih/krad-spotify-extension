use super::super::process::{self, output_within};
use super::files::{self, Files};
use super::names::AppNames;
use super::payload::{Payload, Update};
use super::snapshot::Snapshot;
use crate::media::MediaError;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const PERL: &str = "/usr/bin/perl";
const ROOT: &str = "/";
const THREAD_NAME: &str = "mediaremote";
const PROGRAM_NAME: &str = "MediaRemote";
const COMMAND_TIMEOUT: Duration = Duration::from_millis(900);
const RESTART_DELAY: Duration = Duration::from_secs(1);
const MAX_RESTART_DELAY: Duration = Duration::from_secs(60);
const HEALTHY_RUN: Duration = Duration::from_secs(30);
const EVALUATE: &str = "-e";
const END_OF_OPTIONS: &str = "--";
const STREAM_ARGS: [&str; 3] = ["stream", "--micros", "--debounce=100"];
const LAUNCHER: &str = "use POSIX ();\n\
    POSIX::close($_) for 3 .. 255;\n\
    my $stream = $$;\n\
    my $watchdog = fork();\n\
    if (defined $watchdog && $watchdog == 0) {\n\
      close STDOUT; close STDERR;\n\
      1 while sysread(STDIN, my $buffer, 64);\n\
      kill 'TERM', $stream if getppid() == $stream;\n\
      exit 0;\n\
    }\n\
    close STDIN;\n\
    my $script = shift @ARGV;\n\
    do $script;\n\
    die $@ if $@;\n";

pub(super) struct Adapter {
    files: Files,
    current: Mutex<Option<Snapshot>>,
}

pub(super) fn start() -> Option<Arc<Adapter>> {
    let adapter = Arc::new(Adapter {
        files: files::extract().ok()?,
        current: Mutex::new(None),
    });
    let streaming = Arc::clone(&adapter);
    thread::Builder::new()
        .name(THREAD_NAME.to_owned())
        .spawn(move || streaming.stream_forever())
        .ok()?;
    Some(adapter)
}

impl Adapter {
    pub(super) fn snapshot(&self) -> Option<Snapshot> {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, snapshot: Option<Snapshot>) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = snapshot;
    }

    pub(super) fn run(&self, args: &[&str]) -> Result<(), MediaError> {
        self.files.ensure()?;
        let mut command = self.perl();
        command.args(args);
        let output = output_within(command, COMMAND_TIMEOUT)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(process::failure(PROGRAM_NAME, &output))
        }
    }

    fn perl(&self) -> Command {
        let mut command = Command::new(PERL);
        command
            .arg(&self.files.script)
            .arg(&self.files.framework)
            .current_dir(ROOT);
        command
    }

    fn stream_forever(&self) {
        let mut delay = RESTART_DELAY;
        let mut names = AppNames::default();
        loop {
            let started = Instant::now();
            self.stream_once(&mut names);
            self.set(None);
            if started.elapsed() >= HEALTHY_RUN {
                delay = RESTART_DELAY;
            }
            thread::sleep(delay);
            delay = (delay * 2).min(MAX_RESTART_DELAY);
        }
    }

    fn stream_once(&self, names: &mut AppNames) {
        if self.files.ensure().is_err() {
            return;
        }
        let Ok(mut child) = self.spawn_stream() else {
            return;
        };
        if let Some(stdout) = child.stdout.take() {
            self.follow(BufReader::new(stdout), names);
        }
        let _ = child.kill();
        drop(child.stdin.take());
        let _ = child.wait();
    }

    fn follow(&self, stream: impl BufRead, names: &mut AppNames) {
        let mut payload = Payload::default();
        let mut artwork = None;
        for line in stream.lines().map_while(Result::ok) {
            let Some(update) = Update::parse(&line) else {
                continue;
            };
            if payload.apply(update) {
                artwork = payload.artwork();
            }
            let snapshot = payload.now_playing().map(|now_playing| {
                let name = names.get(&now_playing.id);
                Snapshot::new(now_playing, name, artwork.clone())
            });
            self.set(snapshot);
        }
    }

    fn spawn_stream(&self) -> std::io::Result<Child> {
        let mut command = Command::new(PERL);
        command
            .args([EVALUATE, LAUNCHER, END_OF_OPTIONS])
            .arg(&self.files.script)
            .arg(&self.files.framework)
            .args(STREAM_ARGS)
            .current_dir(ROOT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        process::spawn(command)
    }
}
