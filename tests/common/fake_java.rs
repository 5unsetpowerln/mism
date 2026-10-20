use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::kill;
use nix::unistd::Pid;

/// What a server started by the `Normal` fake `java` prints to its stdout.
pub fn server_stdout_line(jar_name: &str) -> String {
    format!("[fake java] starting {jar_name}")
}

/// What a server started by the `Normal` fake `java` prints to its stderr.
pub const SERVER_STDERR_LINE: &str = "[fake java] no eula.txt, so writing it and stopping";

/// The `server.properties` the `Normal` fake server writes.
pub const GENERATED_SERVER_PROPERTIES: &str =
    "motd=A Minecraft Server\nserver-port=25565\nonline-mode=true\n";

/// The `eula.txt` the `Normal` fake server writes: a comment, `eula=false`, and a trailing line.
pub const GENERATED_EULA_TXT: &str = "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\neula=false\n#written by the fake java\n";

const JAVA_25_VERSION_LINE: &str = r#"openjdk version "25.0.1" 2025-10-21"#;
const JAVA_21_VERSION_LINE: &str = r#"openjdk version "21.0.4" 2024-07-16"#;
const INVOCATION_LOG: &str = "invocations.log";
const SERVER_PID_FILE: &str = "java.pid";
const RELEASE_FILE: &str = "release";
const SERVER_PID_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy)]
pub enum Behavior {
    /// Java 25. The server prints a line to stdout and one to stderr, writes `server.properties`
    /// and an `eula.txt` with `eula=false`, and exits 0.
    Normal,
    /// Java 25. The server exits 0 without writing anything.
    WritesNothing,
    /// Java 25. The server records its process ID in `java.pid` and then sleeps for 10 minutes.
    Hangs,
    /// Java 25. The server records its process ID in `java.pid`, waits until
    /// `FakeJava::release_server` is called, and then behaves as with `Normal`. It waits at most
    /// a minute, so that a test that fails before releasing it leaves no endless process.
    WaitsForRelease,
    /// Java 21. The server behaves as with `Normal`.
    OldVersion,
}

/// A directory holding one fake `java` script, to be put first on `PATH`. The script appends the
/// arguments of each invocation to `invocations.log` in that directory, one invocation per line.
pub struct FakeJava {
    directory: PathBuf,
}

impl FakeJava {
    /// Creates `directory` and writes the `java` script with `behavior` into it.
    pub fn create(directory: PathBuf, behavior: Behavior) -> FakeJava {
        fs::create_dir(&directory).unwrap();
        write_executable(&directory.join("java"), &script(&directory, behavior));
        FakeJava { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The arguments of each invocation so far, joined by spaces, in invocation order.
    pub fn invocations(&self) -> Vec<String> {
        match fs::read_to_string(self.directory.join(INVOCATION_LOG)) {
            Ok(log) => log.lines().map(str::to_owned).collect(),
            Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("failed to read the invocation log: {error}"),
        }
    }

    pub fn invocation_count(&self) -> usize {
        self.invocations().len()
    }

    /// Waits until a `Hangs` or `WaitsForRelease` server has recorded its process ID, and returns
    /// it.
    pub fn wait_for_server_pid(&self) -> Pid {
        let path = self.directory.join(SERVER_PID_FILE);
        let deadline = Instant::now() + SERVER_PID_DEADLINE;
        loop {
            match fs::read_to_string(&path) {
                Ok(pid) => return Pid::from_raw(pid.trim().parse().unwrap()),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => panic!("failed to read {}: {error}", path.display()),
            }
            if Instant::now() > deadline {
                panic!("the fake server did not start within {SERVER_PID_DEADLINE:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Lets a waiting `WaitsForRelease` server continue.
    pub fn release_server(&self) {
        fs::write(self.directory.join(RELEASE_FILE), "").unwrap();
    }
}

/// Whether a process with this ID exists (a zombie counts as existing).
pub fn is_alive(pid: Pid) -> bool {
    kill(pid, None).is_ok()
}

fn script(directory: &Path, behavior: Behavior) -> String {
    let version_line = match behavior {
        Behavior::OldVersion => JAVA_21_VERSION_LINE,
        Behavior::Normal
        | Behavior::WritesNothing
        | Behavior::Hangs
        | Behavior::WaitsForRelease => JAVA_25_VERSION_LINE,
    };
    let server = match behavior {
        Behavior::Normal | Behavior::OldVersion => normal_server(),
        Behavior::WritesNothing => ":".to_owned(),
        // `exec` keeps the PID for `sleep`, so a signal sent to that PID reaches the sleeping
        // process.
        Behavior::Hangs => format!("{}\nexec sleep 600", record_server_pid()),
        Behavior::WaitsForRelease => format!(
            "{}\n\
             waited=0\n\
             while [ ! -e \"$directory/{RELEASE_FILE}\" ] && [ $waited -lt 600 ]; do\n\
             sleep 0.1\n\
             waited=$((waited + 1))\n\
             done\n\
             {}",
            record_server_pid(),
            normal_server()
        ),
    };
    format!(
        "#!/bin/sh\n\
         directory='{}'\n\
         echo \"$*\" >> \"$directory/{INVOCATION_LOG}\"\n\
         case \"$1\" in\n\
         -version) echo '{version_line}' >&2 ;;\n\
         -jar)\n{server}\n;;\n\
         esac\n",
        directory.display()
    )
}

/// The commands of the `Normal` server: it prints a line to stdout and one to stderr, and writes
/// `server.properties` and `eula.txt`.
fn normal_server() -> String {
    format!(
        "echo \"[fake java] starting $2\"\n\
         echo '{SERVER_STDERR_LINE}' >&2\n\
         printf '%s' '{GENERATED_SERVER_PROPERTIES}' > server.properties\n\
         printf '%s' '{GENERATED_EULA_TXT}' > eula.txt"
    )
}

/// Writes the server's process ID to `java.pid` through a temporary file, so the PID file appears
/// only complete.
fn record_server_pid() -> String {
    format!(
        "echo $$ > \"$directory/{SERVER_PID_FILE}.tmp\"\n\
         mv \"$directory/{SERVER_PID_FILE}.tmp\" \"$directory/{SERVER_PID_FILE}\""
    )
}

/// Writes an executable file through a child process. Had this process opened the file for
/// writing, a child forked meanwhile by a parallel test could inherit the descriptor and make
/// executing the script fail with ETXTBSY.
fn write_executable(path: &Path, content: &str) {
    let mut writer = Command::new("sh")
        .args(["-c", r#"cat > "$0" && chmod +x "$0""#])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    writer
        .stdin
        .take()
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();
    let status = writer.wait().unwrap();
    if !status.success() {
        panic!("writing {} failed: {status}", path.display());
    }
}
