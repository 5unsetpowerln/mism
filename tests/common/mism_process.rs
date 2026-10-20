use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

const EXIT_DEADLINE: Duration = Duration::from_secs(30);

pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Runs the built `mism` binary with a clean environment.
pub struct MismProcess {
    command: Command,
}

impl MismProcess {
    pub fn new() -> MismProcess {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mism"));
        command
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        MismProcess { command }
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

    /// Runs mism to its exit, failing the test if that takes longer than 30 seconds.
    pub fn run(self) -> Output {
        self.spawn().wait()
    }

    /// Starts mism. Its stdin is a pipe that stays open and receives nothing, so mism waiting for
    /// input would never exit. The command, with the parent's copy of a descriptor given to
    /// `stdout`, is dropped here.
    pub fn spawn(mut self) -> RunningMism {
        let child = self.command.spawn().unwrap();
        RunningMism { child }
    }
}

pub struct RunningMism {
    child: Child,
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
        Output {
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
            exit_code: output.status.code().expect("mism was killed by a signal"),
        }
    }
}
