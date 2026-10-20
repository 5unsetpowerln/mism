use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::Signal::{SIGKILL, SIGTERM};
use nix::sys::signal::killpg;
use nix::unistd::{AccessFlags, Pid, access};

use crate::interruption::{self, Signal};
use crate::output;

const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// How long an interrupted server may take to exit after SIGTERM before it gets SIGKILL.
const TERMINATION_GRACE_PERIOD: Duration = Duration::from_secs(10);
const EULA_FILE_NAME: &str = "eula.txt";
const SERVER_PROPERTIES_FILE_NAME: &str = "server.properties";
const EULA_DISAGREED: &str = "eula=false";
const EULA_AGREED: &str = "eula=true";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("`java` was not found on PATH")]
    JavaNotFound { minimum_java_version: u32 },
    #[error("failed to run `{} -version`", java.display())]
    RunJavaVersion {
        java: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{} -version` failed ({status}):\n{output}", java.display())]
    JavaVersionFailed {
        java: PathBuf,
        status: ExitStatus,
        output: String,
    },
    #[error("the output of `{} -version` names no Java version:\n{output}", java.display())]
    JavaVersionUnrecognized { java: PathBuf, output: String },
    #[error("{} is Java {found}, older than the required Java {minimum}", java.display())]
    JavaTooOld {
        java: PathBuf,
        found: u32,
        minimum: u32,
    },
    #[error("failed to start the server with {} in {}", java.display(), working_directory.display())]
    StartServer {
        java: PathBuf,
        working_directory: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to wait for the server to exit")]
    WaitServer {
        #[source]
        source: io::Error,
    },
    #[error("failed to send {signal} to the server")]
    SignalServer {
        signal: nix::sys::signal::Signal,
        #[source]
        source: Errno,
    },
    #[error("failed to check whether {} exists", path.display())]
    CheckGeneratedFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "the server exited ({status}) without creating both {EULA_FILE_NAME} and {SERVER_PROPERTIES_FILE_NAME} in {}",
        working_directory.display()
    )]
    InitialLaunchFailed {
        working_directory: PathBuf,
        status: ExitStatus,
    },
    #[error("failed to read {}", path.display())]
    ReadEula {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} has no `{EULA_DISAGREED}` to change to `{EULA_AGREED}`", path.display())]
    EulaSettingMissing { path: PathBuf },
    #[error("failed to write {}", path.display())]
    WriteEula {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Output(#[from] output::Error),
    #[error("interrupted by {0}")]
    Interrupted(Signal),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::JavaNotFound {
                minimum_java_version,
            } => Some(format!(
                "install Java {minimum_java_version} or newer and add the directory of its `java` to PATH"
            )),
            Error::JavaTooOld { minimum, .. } => Some(format!(
                "install Java {minimum} or newer and put the directory of its `java` first on PATH"
            )),
            Error::InitialLaunchFailed { .. } => {
                Some("see the server log printed above for why the server stopped".to_owned())
            }
            Error::Output(error) => error.hint(),
            _ => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interrupted(signal) => Some(*signal),
            _ => None,
        }
    }
}

/// Runs the server once in `working_directory`, where it generates `eula.txt` and
/// `server.properties` and exits, then agrees to the EULA in `eula.txt`.
pub fn launch(
    working_directory: &Path,
    jar_name: &str,
    minimum_java_version: u32,
) -> Result<(), Error> {
    // An unset PATH has no directory to search.
    let path_variable = env::var_os("PATH").unwrap_or_default();
    launch_with_path_variable(
        &path_variable,
        working_directory,
        jar_name,
        minimum_java_version,
    )
}

fn launch_with_path_variable(
    path_variable: &OsStr,
    working_directory: &Path,
    jar_name: &str,
    minimum_java_version: u32,
) -> Result<(), Error> {
    let java = find_java(path_variable).ok_or(Error::JavaNotFound {
        minimum_java_version,
    })?;
    check_java_version(&java, minimum_java_version)?;
    let status = run_server(&java, working_directory, jar_name)?;
    check_generated_files(working_directory, status)?;
    agree_to_eula(working_directory)
}

/// The first executable file named `java` in the absolute directories of `path_variable`.
/// Empty and relative entries would make the result depend on the current directory, so they are
/// not searched.
fn find_java(path_variable: &OsStr) -> Option<PathBuf> {
    env::split_paths(path_variable)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join("java"))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file() && access(path, AccessFlags::X_OK).is_ok()
}

fn check_java_version(java: &Path, minimum: u32) -> Result<(), Error> {
    let found = java_major_version(java)?;
    if found < minimum {
        return Err(Error::JavaTooOld {
            java: java.to_path_buf(),
            found,
            minimum,
        });
    }
    Ok(())
}

fn java_major_version(java: &Path) -> Result<u32, Error> {
    let output = Command::new(java)
        .arg("-version")
        .output()
        .map_err(|source| Error::RunJavaVersion {
            java: java.to_path_buf(),
            source,
        })?;
    // A Ctrl-C also reaches `java -version`, which shares mism's process group, so whatever it
    // printed is reported as the interruption.
    if let Some(signal) = interruption::received() {
        return Err(Error::Interrupted(signal));
    }
    let printed = String::from_utf8_lossy(&output.stderr)
        .trim_end()
        .to_owned();
    if !output.status.success() {
        return Err(Error::JavaVersionFailed {
            java: java.to_path_buf(),
            status: output.status,
            output: printed,
        });
    }
    parse_java_major_version(&printed).ok_or_else(|| Error::JavaVersionUnrecognized {
        java: java.to_path_buf(),
        output: printed,
    })
}

/// Reads the major version from what `java -version` prints, whose version line looks like
/// `openjdk version "25.0.1" 2025-10-21`, or `java version "1.8.0_392"` before Java 9.
fn parse_java_major_version(printed: &str) -> Option<u32> {
    let version = printed.lines().find_map(quoted_version)?;
    leading_number(version.strip_prefix("1.").unwrap_or(version))
}

fn quoted_version(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once("version \"")?;
    rest.split_once('"').map(|(version, _)| version)
}

fn leading_number(text: &str) -> Option<u32> {
    let end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    text[..end].parse().ok()
}

/// How the server's process ended.
enum ServerExit {
    /// The server exited by itself.
    Exited(ExitStatus),
    /// mism stopped the server because this signal arrived.
    Stopped(Signal),
}

/// Steps 3 and 4: runs the server with its output between the line announcing it and the line
/// ending it (out-2), and returns the server's exit status.
fn run_server(java: &Path, working_directory: &Path, jar_name: &str) -> Result<ExitStatus, Error> {
    output::write_external_output_start(jar_name)?;
    let mut server = start_server(java, working_directory, jar_name)?;
    let exit = wait_for_exit(&mut server)?;
    output::write_external_output_end(jar_name)?;
    match exit {
        ServerExit::Exited(status) => Ok(status),
        ServerExit::Stopped(signal) => Err(Error::Interrupted(signal)),
    }
}

fn start_server(java: &Path, working_directory: &Path, jar_name: &str) -> Result<Child, Error> {
    Command::new(java)
        .args(["-jar", jar_name, "--nogui"])
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(io::stdout())
        .stderr(io::stdout())
        // In its own process group, the server does not receive the terminal's Ctrl-C, so mism
        // decides when and how it stops.
        .process_group(0)
        .spawn()
        .map_err(|source| Error::StartServer {
            java: java.to_path_buf(),
            working_directory: working_directory.to_path_buf(),
            source,
        })
}

/// Waits without a time limit, but stops the server once SIGINT or SIGTERM has arrived.
fn wait_for_exit(server: &mut Child) -> Result<ServerExit, Error> {
    loop {
        if let Some(signal) = interruption::received() {
            stop_server(server, TERMINATION_GRACE_PERIOD)?;
            return Ok(ServerExit::Stopped(signal));
        }
        if let Some(status) = exit_status(server)? {
            return Ok(ServerExit::Exited(status));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Sends SIGTERM to the server's process group, and SIGKILL if the server has not exited after
/// `grace_period`. Returns once the server has exited.
fn stop_server(server: &mut Child, grace_period: Duration) -> Result<(), Error> {
    signal_process_group(server, SIGTERM)?;
    if wait_until(server, Instant::now() + grace_period)?.is_none() {
        signal_process_group(server, SIGKILL)?;
        server
            .wait()
            .map_err(|source| Error::WaitServer { source })?;
    }
    Ok(())
}

/// The server's exit status, or `None` if it is still running at `deadline`.
fn wait_until(server: &mut Child, deadline: Instant) -> Result<Option<ExitStatus>, Error> {
    loop {
        let status = exit_status(server)?;
        if status.is_some() || Instant::now() >= deadline {
            return Ok(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn exit_status(server: &mut Child) -> Result<Option<ExitStatus>, Error> {
    server
        .try_wait()
        .map_err(|source| Error::WaitServer { source })
}

/// The server leads its own process group (`start_server`), so the group's ID is its process ID.
/// Until the server is waited for, the group exists, at least as the server's zombie.
fn signal_process_group(server: &Child, signal: nix::sys::signal::Signal) -> Result<(), Error> {
    let group = Pid::from_raw(i32::try_from(server.id()).expect("a process ID fits in pid_t"));
    killpg(group, signal).map_err(|source| Error::SignalServer { signal, source })
}

fn check_generated_files(working_directory: &Path, status: ExitStatus) -> Result<(), Error> {
    for name in [EULA_FILE_NAME, SERVER_PROPERTIES_FILE_NAME] {
        let path = working_directory.join(name);
        let exists = path
            .try_exists()
            .map_err(|source| Error::CheckGeneratedFile { path, source })?;
        if !exists {
            return Err(Error::InitialLaunchFailed {
                working_directory: working_directory.to_path_buf(),
                status,
            });
        }
    }
    Ok(())
}

fn agree_to_eula(working_directory: &Path) -> Result<(), Error> {
    let path = working_directory.join(EULA_FILE_NAME);
    let generated = fs::read_to_string(&path).map_err(|source| Error::ReadEula {
        path: path.clone(),
        source,
    })?;
    let agreed = with_eula_agreed(&generated)
        .ok_or_else(|| Error::EulaSettingMissing { path: path.clone() })?;
    fs::write(&path, agreed).map_err(|source| Error::WriteEula { path, source })
}

/// `eula_txt` with each `eula=false` replaced by `eula=true` and every other byte unchanged, or
/// `None` when it has no `eula=false`.
fn with_eula_agreed(eula_txt: &str) -> Option<String> {
    eula_txt
        .contains(EULA_DISAGREED)
        .then(|| eula_txt.replace(EULA_DISAGREED, EULA_AGREED))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Write;
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use nix::sys::signal::Signal::{SIGKILL, SIGTERM};
    use tempfile::TempDir;

    use super::*;

    const JAVA_25: &str = r#"openjdk version "25.0.1" 2025-10-21"#;
    const JAVA_21: &str = r#"openjdk version "21.0.9" 2025-10-21"#;
    const GENERATE_FILES: &str = "printf 'eula=false\\n' > eula.txt; : > server.properties";
    const JAR_NAME: &str = "paper-26.3-206.jar";

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    /// Writes an executable script through a child process. Had this process opened the file for
    /// writing, a child forked meanwhile by a parallel test could inherit the descriptor and make
    /// executing the script fail with ETXTBSY.
    fn write_script(path: &Path, content: &str) {
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

    /// A directory holding a fake `java` that prints `version_line` to stderr for `-version` and
    /// runs the shell commands `on_jar` for `-jar`.
    fn fake_java(version_line: &str, on_jar: &str) -> TempDir {
        let directory = temporary_directory();
        write_script(
            &directory.path().join("java"),
            &format!(
                "#!/bin/sh\ncase \"$1\" in\n-version) echo '{version_line}' >&2 ;;\n-jar) {on_jar} ;;\nesac\n"
            ),
        );
        directory
    }

    fn launch_with_fake_java(java: &TempDir, working_directory: &TempDir) -> Result<(), Error> {
        launch_with_path_variable(
            java.path().as_os_str(),
            working_directory.path(),
            JAR_NAME,
            25,
        )
    }

    fn file_contents(directory: &Path) -> BTreeMap<String, String> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().into_string().unwrap(),
                    fs::read_to_string(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    fn wait_for_file(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            if Instant::now() > deadline {
                panic!("{} did not appear", path.display());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn exit_signal(server: &mut Child) -> Option<i32> {
        server
            .try_wait()
            .unwrap()
            .and_then(|status| status.signal())
    }

    /// d-launch-4
    #[test]
    fn agreeing_to_the_eula_changes_only_the_eula_setting() {
        let generated = "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\n#Tue Oct 20 12:00:00 JST 2026\neula=false\n";

        let agreed = with_eula_agreed(generated);

        assert_eq!(
            agreed.as_deref(),
            Some(
                "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\n#Tue Oct 20 12:00:00 JST 2026\neula=true\n"
            )
        );
    }

    /// 「初回起動」 方法 6: a text without `eula=false` has nothing to agree to.
    #[test]
    fn text_without_eula_false_is_not_agreed() {
        assert_eq!(with_eula_agreed("eula=FALSE\n"), None);
    }

    /// 「初回起動」 方法 2: the major version of an OpenJDK release is its first number.
    #[test]
    fn openjdk_version_output_gives_the_first_number() {
        let output = "openjdk version \"25.0.1\" 2025-10-21\nOpenJDK Runtime Environment (build 25.0.1+8-27)\nOpenJDK 64-Bit Server VM (build 25.0.1+8-27, mixed mode, sharing)\n";

        assert_eq!(parse_java_major_version(output), Some(25));
    }

    /// 「初回起動」 方法 2: the major version of a `1.x` release is the number after `1.`.
    #[test]
    fn legacy_version_output_gives_the_number_after_one() {
        let output = "java version \"1.8.0_392\"\nJava(TM) SE Runtime Environment (build 1.8.0_392-b08)\nJava HotSpot(TM) 64-Bit Server VM (build 25.392-b08, mixed mode)\n";

        assert_eq!(parse_java_major_version(output), Some(8));
    }

    /// 「初回起動」 方法 2: the JVM may print notes such as `Picked up ...` before the version.
    #[test]
    fn version_output_after_a_note_gives_the_version() {
        let output = "Picked up JAVA_TOOL_OPTIONS: -Xmx2G\nopenjdk version \"21.0.9\" 2025-10-21\n";

        assert_eq!(parse_java_major_version(output), Some(21));
    }

    /// 「初回起動」 方法 2: output without a version is not read as one.
    #[test]
    fn garbage_output_gives_no_version() {
        assert_eq!(
            parse_java_major_version("Error: could not open `java.cfg'\n"),
            None
        );
    }

    /// 「初回起動」 方法 1: `java` is found in a later `PATH` entry.
    #[test]
    fn find_java_returns_the_executable_in_path() {
        let empty = temporary_directory();
        let java = fake_java(JAVA_25, GENERATE_FILES);
        let path_variable = std::env::join_paths([empty.path(), java.path()]).unwrap();

        assert_eq!(find_java(&path_variable), Some(java.path().join("java")));
    }

    /// 「初回起動」 方法 1: a `PATH` without `java` gives nothing.
    #[test]
    fn find_java_without_java_in_path_returns_none() {
        let empty = temporary_directory();

        assert_eq!(find_java(empty.path().as_os_str()), None);
    }

    /// 「初回起動」 方法 1: a `java` file that is not executable is not a `java` to run.
    #[test]
    fn find_java_skips_a_non_executable_file() {
        let directory = temporary_directory();
        fs::write(directory.path().join("java"), "#!/bin/sh\n").unwrap();

        assert_eq!(find_java(directory.path().as_os_str()), None);
    }

    /// 「初回起動」 方法 3 to 6: after the server generated its files, `eula.txt` agrees to the
    /// EULA and `server.properties` is kept.
    #[test]
    fn launch_leaves_server_properties_and_an_agreed_eula() {
        let java = fake_java(JAVA_25, GENERATE_FILES);
        let working_directory = temporary_directory();

        launch_with_fake_java(&java, &working_directory).unwrap();

        assert_eq!(
            file_contents(working_directory.path()),
            BTreeMap::from([
                ("eula.txt".to_owned(), "eula=true\n".to_owned()),
                ("server.properties".to_owned(), String::new()),
            ])
        );
    }

    /// 「初回起動」 方法 3: the server runs as `java -jar <jar> --nogui` in the working directory.
    #[test]
    fn launch_runs_the_jar_in_the_working_directory() {
        let java = fake_java(
            JAVA_25,
            &format!("echo \"$@\" > arguments; {GENERATE_FILES}"),
        );
        let working_directory = temporary_directory();

        launch_with_fake_java(&java, &working_directory).unwrap();

        assert_eq!(
            fs::read_to_string(working_directory.path().join("arguments")).unwrap(),
            format!("-jar {JAR_NAME} --nogui\n")
        );
    }

    /// 「初回起動」 方法: success depends on the generated files, not on the server's exit status.
    #[test]
    fn launch_succeeds_when_the_server_exits_with_failure_after_generating_files() {
        let java = fake_java(JAVA_25, &format!("{GENERATE_FILES}; exit 1"));
        let working_directory = temporary_directory();

        let result = launch_with_fake_java(&java, &working_directory);

        assert!(result.is_ok(), "{result:?}");
    }

    /// Relied on by d-launch-1: without `java` on `PATH`, launching reports `JavaNotFound`.
    #[test]
    fn launch_without_java_reports_java_not_found() {
        let empty = temporary_directory();
        let working_directory = temporary_directory();

        let result = launch_with_path_variable(
            empty.path().as_os_str(),
            working_directory.path(),
            JAR_NAME,
            25,
        );

        assert!(
            matches!(
                result,
                Err(Error::JavaNotFound {
                    minimum_java_version: 25
                })
            ),
            "{result:?}"
        );
    }

    /// Relied on by d-launch-2: a Java older than the minimum is reported as `JavaTooOld`.
    #[test]
    fn launch_with_old_java_reports_java_too_old() {
        let java = fake_java(JAVA_21, GENERATE_FILES);
        let working_directory = temporary_directory();

        let result = launch_with_fake_java(&java, &working_directory);

        assert!(
            matches!(
                result,
                Err(Error::JavaTooOld {
                    found: 21,
                    minimum: 25,
                    ..
                })
            ),
            "{result:?}"
        );
    }

    /// Relied on by d-launch-3: a server that generates no file makes launching fail.
    #[test]
    fn launch_whose_server_generates_nothing_reports_initial_launch_failed() {
        let java = fake_java(JAVA_25, ":");
        let working_directory = temporary_directory();

        let result = launch_with_fake_java(&java, &working_directory);

        assert!(
            matches!(result, Err(Error::InitialLaunchFailed { .. })),
            "{result:?}"
        );
    }

    /// err-7: a server that generates `eula.txt` but not `server.properties` makes launching fail.
    #[test]
    fn launch_whose_server_generates_only_eula_reports_initial_launch_failed() {
        let java = fake_java(JAVA_25, "printf 'eula=false\\n' > eula.txt");
        let working_directory = temporary_directory();

        let result = launch_with_fake_java(&java, &working_directory);

        assert!(
            matches!(result, Err(Error::InitialLaunchFailed { .. })),
            "{result:?}"
        );
    }

    /// 「mism new の手順と中断」 方法: stopping the server sends SIGTERM to its process group.
    #[test]
    fn stop_server_ends_a_server_with_sigterm() {
        let java = fake_java(JAVA_25, "exec sleep 60");
        let working_directory = temporary_directory();
        let mut server = start_server(
            &java.path().join("java"),
            working_directory.path(),
            JAR_NAME,
        )
        .unwrap();

        stop_server(&mut server, Duration::from_secs(60)).unwrap();

        assert_eq!(exit_signal(&mut server), Some(SIGTERM as i32));
    }

    /// 「mism new の手順と中断」 方法: a server still running after the grace period gets SIGKILL.
    #[test]
    fn stop_server_kills_a_server_that_ignores_sigterm() {
        let java = fake_java(JAVA_25, "trap '' TERM; : > ready; exec sleep 60");
        let working_directory = temporary_directory();
        let mut server = start_server(
            &java.path().join("java"),
            working_directory.path(),
            JAR_NAME,
        )
        .unwrap();
        wait_for_file(&working_directory.path().join("ready"));

        stop_server(&mut server, Duration::from_millis(100)).unwrap();

        assert_eq!(exit_signal(&mut server), Some(SIGKILL as i32));
    }
}
