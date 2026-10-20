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

/// Runs the built `mmmm` binary with a clean environment.
pub struct MmmmProcess {
    command: Command,
}

impl MmmmProcess {
    pub fn new() -> MmmmProcess {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mmmm"));
        command.env_clear();
        MmmmProcess { command }
    }

    pub fn arguments<I, S>(mut self, arguments: I) -> MmmmProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.command.args(arguments.into_iter().map(Into::into));
        self
    }

    pub fn environment(mut self, name: &str, value: impl Into<OsString>) -> MmmmProcess {
        self.command.env(name, value.into());
        self
    }

    pub fn working_directory(mut self, directory: impl Into<PathBuf>) -> MmmmProcess {
        self.command.current_dir(directory.into());
        self
    }

    /// Runs mmmm to its exit, failing the test if that takes longer than 30 seconds.
    pub fn run(self) -> Output {
        self.spawn().wait()
    }

    /// Starts mmmm. Its stdin is a pipe that stays open and receives nothing, so mmmm waiting for
    /// input would never exit.
    pub fn spawn(mut self) -> RunningMmmm {
        let child = self
            .command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        RunningMmmm { child }
    }
}

pub struct RunningMmmm {
    child: Child,
}

impl RunningMmmm {
    pub fn send(&self, signal: Signal) {
        let pid = Pid::from_raw(i32::try_from(self.child.id()).unwrap());
        kill(pid, signal).unwrap();
    }

    /// Kills mmmm with SIGKILL and reaps it. Its output is not read: a server it started may
    /// outlive it and hold the pipes open.
    pub fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    /// Waits for mmmm to exit, failing the test if that takes longer than 30 seconds.
    pub fn wait(mut self) -> Output {
        let deadline = Instant::now() + EXIT_DEADLINE;
        while self.child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                self.child.kill().unwrap();
                panic!("mmmm did not exit within {EXIT_DEADLINE:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = self.child.wait_with_output().unwrap();
        Output {
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
            exit_code: output.status.code().expect("mmmm was killed by a signal"),
        }
    }
}
