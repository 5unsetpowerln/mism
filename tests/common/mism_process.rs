use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use super::stdin_connection::StdinConnection;

const EXIT_DEADLINE: Duration = Duration::from_secs(30);
const STDOUT_DEADLINE: Duration = Duration::from_secs(30);

pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Runs the built `mism` binary with a clean environment.
pub struct MismProcess {
    command: Command,
    stdin: StdinConnection,
}

impl MismProcess {
    pub fn new() -> MismProcess {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mism"));
        command
            .env_clear()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        MismProcess {
            command,
            stdin: StdinConnection::NeverClosing,
        }
    }

    pub fn arguments<I, S>(mut self, arguments: I) -> MismProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.command.args(arguments.into_iter().map(Into::into));
        self
    }

    pub fn environment(mut self, name: &str, value: impl Into<OsString>) -> MismProcess {
        self.command.env(name, value.into());
        self
    }

    pub fn working_directory(mut self, directory: impl Into<PathBuf>) -> MismProcess {
        self.command.current_dir(directory.into());
        self
    }

    /// Connects mism's stdout to `stdout` instead of a pipe. `Output::stdout` is then empty.
    pub fn stdout(mut self, stdout: impl Into<Stdio>) -> MismProcess {
        self.command.stdout(stdout);
        self
    }

    /// Connects mism's stdin as `stdin` says, instead of a pipe that stays open and empty.
    pub fn stdin(mut self, stdin: StdinConnection) -> MismProcess {
        self.stdin = stdin;
        self
    }

    /// Runs mism to its exit, failing the test if that takes longer than 30 seconds.
    pub fn run(self) -> Output {
        self.spawn().wait()
    }

    /// Starts mism. Unless `stdin` connected it otherwise, its stdin is a pipe that stays open
    /// and receives nothing, so mism waiting for input would never exit. The command, with the
    /// parent's copy of a descriptor given to `stdout` or of a terminal's slave side, is dropped
    /// here.
    pub fn spawn(mut self) -> RunningMism {
        let terminal_master = self.stdin.connect(&mut self.command);
        let mut child = self.command.spawn().unwrap();
        self.stdin.feed(&mut child);
        RunningMism {
            child,
            _terminal_master: terminal_master,
            stdout: None,
        }
    }
}

pub struct RunningMism {
    child: Child,
    /// Kept open so that mism's terminal stays open while it runs.
    _terminal_master: Option<File>,
    /// What a thread has read from mism's stdout so far, once `wait_for_stdout` started it.
    stdout: Option<StdoutReader>,
}

struct StdoutReader {
    read: Arc<Mutex<Vec<u8>>>,
    thread: JoinHandle<()>,
}

impl RunningMism {
    pub fn send(&self, signal: Signal) {
        let pid = Pid::from_raw(i32::try_from(self.child.id()).unwrap());
        kill(pid, signal).unwrap();
    }

    /// Kills mism with SIGKILL and reaps it. Its output is not read: a server it started may
    /// outlive it and hold the pipes open.
    pub fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    /// Waits until mism's stdout contains `text`, failing the test after 30 seconds. `wait`
    /// still returns all of stdout.
    pub fn wait_for_stdout(&mut self, text: &str) {
        let stdout = self.stdout.get_or_insert_with(|| {
            let mut pipe = self.child.stdout.take().expect("stdout is a pipe");
            let read = Arc::new(Mutex::new(Vec::new()));
            let shared = Arc::clone(&read);
            let thread = thread::spawn(move || {
                let mut buffer = [0; 4096];
                while let Ok(count @ 1..) = pipe.read(&mut buffer) {
                    shared.lock().unwrap().extend_from_slice(&buffer[..count]);
                }
            });
            StdoutReader { read, thread }
        });
        let deadline = Instant::now() + STDOUT_DEADLINE;
        while !String::from_utf8_lossy(&stdout.read.lock().unwrap()).contains(text) {
            assert!(
                Instant::now() < deadline,
                "stdout did not contain {text:?} within {STDOUT_DEADLINE:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits for mism to exit, failing the test if that takes longer than 30 seconds.
    pub fn wait(mut self) -> Output {
        let deadline = Instant::now() + EXIT_DEADLINE;
        while self.child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                self.child.kill().unwrap();
                panic!("mism did not exit within {EXIT_DEADLINE:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = self.child.wait_with_output().unwrap();
        let stdout = match self.stdout {
            Some(reader) => {
                reader.thread.join().unwrap();
                Arc::try_unwrap(reader.read).unwrap().into_inner().unwrap()
            }
            None => output.stdout,
        };
        Output {
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
            exit_code: output.status.code().expect("mism was killed by a signal"),
        }
    }
}
