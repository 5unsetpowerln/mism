mod common;

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::time::Duration;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::{Value, json};

use common::fake_java::{self, Behavior};
use common::fake_papermc;
use common::mism_process::{Output, RunningMism};
use common::new_server_environment::{JAR_BYTES, NewServerEnvironment};
use common::server_output::{after_server_output, server_output};
use common::slow_jar_server::SlowJarServer;

const JAR_NAME: &str = "paper-26.3-206.jar";
const IDLE_TIMEOUT_VARIABLE: &str = "MISM_DOWNLOAD_IDLE_TIMEOUT";

/// Runs `mism new <software> <the environment's server path> <options>`.
fn run_new(environment: &NewServerEnvironment, software: &str, options: &[&str]) -> Output {
    let mut arguments = vec![
        OsString::from(software),
        environment.server_path().into_os_string(),
    ];
    arguments.extend(options.iter().map(OsString::from));
    environment.mism_new(arguments).run()
}

/// Starts `mism new paper <the environment's server path>`.
fn spawn_new_paper(environment: &NewServerEnvironment) -> RunningMism {
    environment
        .mism_new([
            OsString::from("paper"),
            environment.server_path().into_os_string(),
        ])
        .spawn()
}

/// Runs `mism new paper <path>` against the saved Paper version list with the fake `java`
/// `behavior`, after `prepare` has changed the environment.
fn create_paper_server_after(
    behavior: Behavior,
    prepare: impl FnOnce(&NewServerEnvironment),
) -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::serving_paper_26_3(behavior);
    prepare(&environment);
    let output = run_new(&environment, "paper", &[]);
    (environment, output)
}

fn create_paper_server(behavior: Behavior) -> (NewServerEnvironment, Output) {
    create_paper_server_after(behavior, |_| {})
}

/// An environment whose fake PaperMC serves `version_list` as Paper's, and `JAR_BYTES` with their
/// true SHA-256 as Paper `minecraft_version` `software_version`.
fn environment_serving(
    version_list: &str,
    minecraft_version: &str,
    software_version: u64,
) -> NewServerEnvironment {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment
        .papermc
        .mount_version_list("paper", version_list);
    environment.papermc.mount_jar_download(
        "paper",
        minecraft_version,
        software_version,
        JAR_BYTES,
        &fake_papermc::sha256_hex(JAR_BYTES),
    );
    environment
}

/// The saved Paper version list with `change` applied to the entry of `minecraft_version`.
fn paper_version_list_with(minecraft_version: &str, change: impl FnOnce(&mut Value)) -> String {
    let mut list = fake_papermc::saved_version_list("paper");
    let entry = list["versions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["version"]["id"] == minecraft_version)
        .unwrap();
    change(entry);
    list.to_string()
}

fn saved_paper_version_list() -> String {
    fake_papermc::saved_version_list("paper").to_string()
}

/// The space-separated fields, after `mism: `, of the last line `mism new` printed to stdout.
fn printed_fields(output: &Output) -> Vec<&str> {
    output
        .stdout
        .lines()
        .last()
        .and_then(|line| line.strip_prefix("mism: "))
        .unwrap_or_else(|| panic!("the last line of stdout lacks `mism: `: {}", output.stdout))
        .split(' ')
        .collect()
}

fn read_created_file(environment: &NewServerEnvironment, name: &str) -> String {
    fs::read_to_string(environment.server_path().join(name)).unwrap()
}

fn read_mism_toml(environment: &NewServerEnvironment) -> toml::Table {
    toml::from_str(&read_created_file(environment, "mism.toml")).unwrap()
}

/// mism's error report: what follows the end of the server's output in stdout if the server ran,
/// or else the whole of stdout.
fn error_report(output: &Output) -> &str {
    after_server_output(&output.stdout, JAR_NAME).unwrap_or(&output.stdout)
}

/// The first line of mism's error report.
fn error_line(output: &Output) -> &str {
    error_report(output)
        .lines()
        .next()
        .unwrap_or_else(|| panic!("stdout has no error: {}", output.stdout))
}

/// Whether the server's output, between the lines of out-2, has the line `expected`.
fn server_printed(output: &Output, expected: &str) -> bool {
    server_output(&output.stdout, JAR_NAME)
        .is_some_and(|printed| printed.lines().any(|line| line == expected))
}

fn error_report_has_line(output: &Output, expected: &str) -> bool {
    error_report(output).lines().any(|line| line == expected)
}

fn nothing_exists_at(path: &Path) -> bool {
    matches!(fs::symlink_metadata(path), Err(error) if error.kind() == ErrorKind::NotFound)
}

/// The names of the entries of `directory` that are mism's working directories.
fn working_directories(directory: &Path) -> Vec<String> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with(".mism-tmp-"))
        .collect()
}

fn create_with_existing_file() -> (NewServerEnvironment, Output) {
    create_paper_server_after(Behavior::Normal, |environment| {
        fs::write(environment.server_path(), "not a server").unwrap();
    })
}

fn create_with_existing_directory() -> (NewServerEnvironment, Output) {
    create_paper_server_after(Behavior::Normal, |environment| {
        fs::create_dir(environment.server_path()).unwrap();
    })
}

fn create_with_dangling_symlink() -> (NewServerEnvironment, Output) {
    create_paper_server_after(Behavior::Normal, |environment| {
        symlink(
            environment.home().join("nowhere"),
            environment.server_path(),
        )
        .unwrap();
    })
}

/// `mism new paper <servers>/missing/survival`, whose parent does not exist.
fn create_under_missing_parent() -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let path = environment.servers().join("missing/survival");
    let output = environment
        .mism_new([OsString::from("paper"), path.into_os_string()])
        .run();
    (environment, output)
}

fn create_requesting(minecraft_version: &str) -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let output = run_new(&environment, "paper", &["--version", minecraft_version]);
    (environment, output)
}

fn create_without_supported_version() -> (NewServerEnvironment, Output) {
    let version_list =
        saved_paper_version_list().replace(r#""status":"SUPPORTED""#, r#""status":"DEPRECATED""#);
    let environment = environment_serving(&version_list, "26.3", 206);
    let output = run_new(&environment, "paper", &[]);
    (environment, output)
}

/// Returns the base URL mism failed to reach too.
fn create_against_unreachable_papermc() -> (NewServerEnvironment, Output, String) {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let base_url = fake_papermc::unreachable_base_url();
    let output = environment
        .mism_new([
            OsString::from("paper"),
            environment.server_path().into_os_string(),
        ])
        .environment("MISM_PAPERMC_URL", &base_url)
        .run();
    (environment, output, base_url)
}

/// Returns the error line expected for the mismatch too.
fn create_with_wrong_published_sha256() -> (NewServerEnvironment, Output, String) {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("paper");
    let published = fake_papermc::sha256_hex(b"some other jar");
    let jar_url = environment
        .papermc
        .mount_jar_download("paper", "26.3", 206, JAR_BYTES, &published);
    let output = run_new(&environment, "paper", &[]);
    let expected = format!(
        "mism: the SHA-256 of {jar_url} is {}, but PaperMC publishes {published}",
        fake_papermc::sha256_hex(JAR_BYTES)
    );
    (environment, output, expected)
}

fn create_with_server_writing_nothing() -> (NewServerEnvironment, Output) {
    create_paper_server(Behavior::WritesNothing)
}

struct InterruptedRun {
    environment: NewServerEnvironment,
    output: Output,
    server_pid: Pid,
}

/// Runs `mism new paper <path>` with the `Hangs` fake `java`, and sends `signal` to mism once the
/// server is running.
fn interrupt_while_the_server_runs(signal: Signal) -> InterruptedRun {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Hangs);
    let mism = spawn_new_paper(&environment);
    let server_pid = environment.fake_java.wait_for_server_pid();
    mism.send(signal);
    let output = mism.wait();
    InterruptedRun {
        environment,
        output,
        server_pid,
    }
}

/// sel-1: through `mism.toml`, the newest SUPPORTED version of the saved list (26.3) is chosen.
#[test]
fn omitted_version_records_the_newest_supported_version() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert_eq!(
        read_mism_toml(&environment)["minecraft_version"].as_str(),
        Some("26.3")
    );
}

/// sel-1: through stdout, newer versions that are not SUPPORTED are passed over.
#[test]
fn omitted_version_skips_newer_versions_that_are_not_supported() {
    let version_list = paper_version_list_with("26.3", |entry| {
        entry["version"]["support"]["status"] = "DEPRECATED".into();
    });
    let environment = environment_serving(&version_list, "26.2", 134);

    let output = run_new(&environment, "paper", &[]);

    assert_eq!(printed_fields(&output)[1], "26.2");
}

/// sel-2: the requested version is chosen, whatever its support status (1.21.11 is UNSUPPORTED).
#[test]
fn requested_version_is_chosen() {
    let environment = environment_serving(&saved_paper_version_list(), "1.21.11", 132);

    let output = run_new(&environment, "paper", &["--version", "1.21.11"]);

    assert_eq!(printed_fields(&output)[1], "1.21.11");
}

/// sel-2: `26.3` matches the listed `26.3` exactly.
#[test]
fn requested_version_with_the_listed_spelling_succeeds() {
    let (_environment, output) = create_requesting("26.3");

    assert_eq!(output.exit_code, 0);
}

/// sel-2: `26.03` is not converted to the listed `26.3`.
#[test]
fn requested_version_with_another_spelling_is_not_distributed() {
    let (_environment, output) = create_requesting("26.03");

    assert_eq!(
        error_line(&output),
        "mism: PaperMC does not distribute paper for Minecraft 26.03"
    );
}

/// sel-3: the largest software version is chosen, not the last one listed.
#[test]
fn largest_software_version_is_chosen() {
    let version_list = paper_version_list_with("26.3", |entry| {
        entry["builds"].as_array_mut().unwrap().reverse();
    });
    let environment = environment_serving(&version_list, "26.3", 206);

    run_new(&environment, "paper", &[]);

    assert_eq!(
        read_mism_toml(&environment)["software_version"].as_integer(),
        Some(206)
    );
}

/// new-1: the server directory is created under the current directory.
#[test]
fn relative_path_is_created_under_the_current_directory() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);

    environment
        .mism_new(["paper", "survival"])
        .working_directory(environment.servers())
        .run();

    assert!(environment.server_path().is_dir());
}

/// new-1: the printed path is the relative path joined to the current directory.
#[test]
fn relative_path_is_printed_as_an_absolute_path() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);

    let output = environment
        .mism_new(["paper", "survival"])
        .working_directory(environment.servers())
        .run();

    assert_eq!(
        printed_fields(&output)[3],
        environment.server_path().display().to_string()
    );
}

/// new-2
#[test]
fn created_path_is_a_directory() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert!(
        fs::symlink_metadata(environment.server_path())
            .unwrap()
            .is_dir()
    );
}

/// new-3: exactly one jar, with the name PaperMC distributes it under.
#[test]
fn server_directory_holds_one_jar_with_the_distributed_name() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    let jars: Vec<String> = fs::read_dir(environment.server_path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".jar"))
        .collect();
    assert_eq!(jars, [JAR_NAME]);
}

/// new-3: the jar is the one PaperMC served.
#[test]
fn jar_in_the_server_directory_is_the_served_jar() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert_eq!(
        fs::read(environment.server_path().join(JAR_NAME)).unwrap(),
        JAR_BYTES
    );
}

/// new-4: the server is started exactly once, as `java -jar <jar> --nogui`.
#[test]
fn server_is_started_once() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    let launches: Vec<String> = environment
        .fake_java
        .invocations()
        .into_iter()
        .filter(|invocation| invocation.starts_with("-jar"))
        .collect();
    assert_eq!(launches, [format!("-jar {JAR_NAME} --nogui")]);
}

/// new-5: the server's stdout reaches mism's stdout, between the lines of out-2.
#[test]
fn server_stdout_is_copied_to_stdout() {
    let (_environment, output) = create_paper_server(Behavior::Normal);

    assert!(server_printed(
        &output,
        &fake_java::server_stdout_line(JAR_NAME)
    ));
}

/// new-5: the server's stderr reaches mism's stdout, between the lines of out-2.
#[test]
fn server_stderr_is_copied_to_stdout() {
    let (_environment, output) = create_paper_server(Behavior::Normal);

    assert!(server_printed(&output, fake_java::SERVER_STDERR_LINE));
}

/// new-6: `eula.txt` contains `eula=true`.
#[test]
fn eula_txt_contains_eula_true() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert!(read_created_file(&environment, "eula.txt").contains("eula=true"));
}

/// new-6: `eula.txt` does not contain `eula=false`.
#[test]
fn eula_txt_does_not_contain_eula_false() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert!(!read_created_file(&environment, "eula.txt").contains("eula=false"));
}

/// new-6: the comment and the trailing line the server wrote are kept as they were.
#[test]
fn eula_txt_keeps_what_else_the_server_wrote() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert_eq!(
        read_created_file(&environment, "eula.txt"),
        fake_java::GENERATED_EULA_TXT.replace("eula=false", "eula=true")
    );
}

/// new-7: stdin stays open and receives nothing (see `MismProcess::spawn`), so a confirmation
/// prompt would keep mism from exiting.
#[test]
fn eula_is_agreed_to_without_input() {
    let (_environment, output) = create_paper_server(Behavior::Normal);

    assert_eq!(output.exit_code, 0);
}

/// new-8
#[test]
fn server_directory_holds_server_properties() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert!(
        environment
            .server_path()
            .join("server.properties")
            .is_file()
    );
}

/// new-9: Paper, its Minecraft version, and its software version are recorded.
#[test]
fn mism_toml_records_the_software_and_its_versions() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    let expected: toml::Table = toml::from_str(
        "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206",
    )
    .unwrap();
    assert_eq!(read_mism_toml(&environment), expected);
}

/// new-9: Folia is recorded as `folia`.
#[test]
fn mism_toml_records_folia() {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("folia");
    environment.papermc.mount_jar_download(
        "folia",
        "26.3",
        3,
        JAR_BYTES,
        &fake_papermc::sha256_hex(JAR_BYTES),
    );

    run_new(&environment, "folia", &[]);

    assert_eq!(
        read_mism_toml(&environment)["software"].as_str(),
        Some("folia")
    );
}

/// new-10: what follows the server's output on stdout is the software, both versions, and the
/// absolute path, then a newline.
#[test]
fn success_prints_the_software_its_versions_and_the_path() {
    let (environment, output) = create_paper_server(Behavior::Normal);

    assert_eq!(
        after_server_output(&output.stdout, JAR_NAME),
        Some(
            format!(
                "mism: paper 26.3 206 {}\n",
                environment.server_path().display()
            )
            .as_str()
        )
    );
}

/// new-10: Folia is printed as `folia`.
#[test]
fn folia_success_prints_folia() {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("folia");
    environment.papermc.mount_jar_download(
        "folia",
        "26.3",
        3,
        JAR_BYTES,
        &fake_papermc::sha256_hex(JAR_BYTES),
    );

    let output = run_new(&environment, "folia", &[]);

    assert_eq!(
        after_server_output(&output.stdout, "folia-26.3-3.jar"),
        Some(
            format!(
                "mism: folia 26.3 3 {}\n",
                environment.server_path().display()
            )
            .as_str()
        )
    );
}

/// new-10: exit status
#[test]
fn success_exits_with_status_0() {
    let (_environment, output) = create_paper_server(Behavior::Normal);

    assert_eq!(output.exit_code, 0);
}

/// err-1: an existing file
#[test]
fn existing_file_at_the_path_is_an_error() {
    let (environment, output) = create_with_existing_file();

    assert_eq!(
        error_line(&output),
        format!(
            "mism: {} already exists",
            environment.server_path().display()
        )
    );
}

/// err-1: an existing directory
#[test]
fn existing_directory_at_the_path_is_an_error() {
    let (environment, output) = create_with_existing_directory();

    assert_eq!(
        error_line(&output),
        format!(
            "mism: {} already exists",
            environment.server_path().display()
        )
    );
}

/// err-1: a symbolic link whose target does not exist
#[test]
fn dangling_symlink_at_the_path_is_an_error() {
    let (environment, output) = create_with_dangling_symlink();

    assert_eq!(
        error_line(&output),
        format!(
            "mism: {} already exists",
            environment.server_path().display()
        )
    );
}

/// err-2
#[test]
fn missing_parent_directory_is_an_error() {
    let (environment, output) = create_under_missing_parent();

    assert_eq!(
        error_line(&output),
        format!(
            "mism: the parent directory {} does not exist",
            environment.servers().join("missing").display()
        )
    );
}

/// err-3
#[test]
fn version_that_is_not_distributed_is_an_error() {
    let (_environment, output) = create_requesting("9.9.9");

    assert_eq!(
        error_line(&output),
        "mism: PaperMC does not distribute paper for Minecraft 9.9.9"
    );
}

/// err-4
#[test]
fn list_without_supported_version_is_an_error() {
    let (_environment, output) = create_without_supported_version();

    assert_eq!(
        error_line(&output),
        "mism: PaperMC lists no supported Minecraft version for paper"
    );
}

/// err-5
#[test]
fn unreachable_papermc_is_an_error() {
    let (_environment, output, base_url) = create_against_unreachable_papermc();

    assert_eq!(
        error_line(&output),
        format!("mism: request to {base_url}/projects/paper/versions failed")
    );
}

/// err-6
#[test]
fn jar_whose_sha256_differs_from_the_published_one_is_an_error() {
    let (_environment, output, expected) = create_with_wrong_published_sha256();

    assert_eq!(error_line(&output), expected);
}

/// err-7, d-launch-3: the server exited without generating either file.
#[test]
fn server_that_creates_neither_file_is_an_error() {
    let (_environment, output) = create_with_server_writing_nothing();

    assert!(error_line(&output).starts_with(
        "mism: the server exited (exit status: 0) without creating both eula.txt and server.properties in "
    ));
}

/// err-8: after err-3
#[test]
fn nothing_exists_at_the_path_after_a_version_that_is_not_distributed() {
    let (environment, _output) = create_requesting("9.9.9");

    assert!(nothing_exists_at(&environment.server_path()));
}

/// err-8: after err-4
#[test]
fn nothing_exists_at_the_path_after_a_list_without_supported_version() {
    let (environment, _output) = create_without_supported_version();

    assert!(nothing_exists_at(&environment.server_path()));
}

/// err-8: after err-5
#[test]
fn nothing_exists_at_the_path_after_unreachable_papermc() {
    let (environment, _output, _base_url) = create_against_unreachable_papermc();

    assert!(nothing_exists_at(&environment.server_path()));
}

/// err-8: after err-6
#[test]
fn nothing_exists_at_the_path_after_a_sha256_mismatch() {
    let (environment, _output, _expected) = create_with_wrong_published_sha256();

    assert!(nothing_exists_at(&environment.server_path()));
}

/// err-8: after err-7
#[test]
fn nothing_exists_at_the_path_after_a_failed_initial_launch() {
    let (environment, _output) = create_with_server_writing_nothing();

    assert!(nothing_exists_at(&environment.server_path()));
}

/// err-8 (「mism new の手順と中断」 方法: a failure in steps 5 to 8 removes the working directory):
/// after err-7 no working directory is left beside the path.
#[test]
fn no_working_directory_is_left_after_a_failed_initial_launch() {
    let (environment, _output) = create_with_server_writing_nothing();

    assert_eq!(
        working_directories(&environment.servers()),
        Vec::<String>::new()
    );
}

/// err-9: exit status after err-1
#[test]
fn existing_path_exits_with_status_1() {
    let (_environment, output) = create_with_existing_file();

    assert_eq!(output.exit_code, 1);
}

/// err-9: exit status after err-2
#[test]
fn missing_parent_exits_with_status_1() {
    let (_environment, output) = create_under_missing_parent();

    assert_eq!(output.exit_code, 1);
}

/// err-9: exit status after err-3
#[test]
fn version_that_is_not_distributed_exits_with_status_1() {
    let (_environment, output) = create_requesting("9.9.9");

    assert_eq!(output.exit_code, 1);
}

/// err-9: exit status after err-4
#[test]
fn list_without_supported_version_exits_with_status_1() {
    let (_environment, output) = create_without_supported_version();

    assert_eq!(output.exit_code, 1);
}

/// err-9: exit status after err-5
#[test]
fn unreachable_papermc_exits_with_status_1() {
    let (_environment, output, _base_url) = create_against_unreachable_papermc();

    assert_eq!(output.exit_code, 1);
}

/// err-9: exit status after err-6
#[test]
fn sha256_mismatch_exits_with_status_1() {
    let (_environment, output, _expected) = create_with_wrong_published_sha256();

    assert_eq!(output.exit_code, 1);
}

/// err-9: exit status after err-7. d-launch-3: the server generated neither file.
#[test]
fn failed_initial_launch_exits_with_status_1() {
    let (_environment, output) = create_with_server_writing_nothing();

    assert_eq!(output.exit_code, 1);
}

/// err-9: the cause of a communication failure (err-5) is reported.
#[test]
fn unreachable_papermc_reports_the_cause() {
    let (_environment, output, _base_url) = create_against_unreachable_papermc();

    assert!(
        output
            .stdout
            .lines()
            .any(|line| line.starts_with("mism: caused by: "))
    );
}

/// err-9: the remedy for a version that is not distributed (err-3)
#[test]
fn version_that_is_not_distributed_has_a_hint() {
    let (_environment, output) = create_requesting("9.9.9");

    assert!(error_report_has_line(
        &output,
        "mism: hint: run `mism versions paper` to see the distributed Minecraft versions"
    ));
}

/// err-9: the remedy for a communication failure (err-5)
#[test]
fn unreachable_papermc_has_a_hint() {
    let (_environment, output, _base_url) = create_against_unreachable_papermc();

    assert!(error_report_has_line(
        &output,
        "mism: hint: check your network connection"
    ));
}

/// err-9: the remedy for a failed initial launch (err-7)
#[test]
fn failed_initial_launch_has_a_hint() {
    let (_environment, output) = create_with_server_writing_nothing();

    assert!(error_report_has_line(
        &output,
        "mism: hint: see the server log printed above for why the server stopped"
    ));
}

/// int-1: SIGINT exits with status 130.
#[test]
fn sigint_exits_with_status_130() {
    let run = interrupt_while_the_server_runs(Signal::SIGINT);

    assert_eq!(run.output.exit_code, 130);
}

/// int-1: SIGINT is reported as the reason mism stopped.
#[test]
fn sigint_is_reported() {
    let run = interrupt_while_the_server_runs(Signal::SIGINT);

    assert_eq!(error_line(&run.output), "mism: interrupted by SIGINT");
}

/// int-1, d-new-3: on SIGINT the server has exited by the time mism exits.
#[test]
fn sigint_stops_the_server() {
    let run = interrupt_while_the_server_runs(Signal::SIGINT);

    assert!(!fake_java::is_alive(run.server_pid));
}

/// int-1: on SIGINT nothing is left at the path.
#[test]
fn sigint_leaves_nothing_at_the_path() {
    let run = interrupt_while_the_server_runs(Signal::SIGINT);

    assert!(nothing_exists_at(&run.environment.server_path()));
}

/// int-1 (「mism new の手順と中断」 方法: the working directory is removed after the server exits):
/// on SIGINT no working directory is left beside the path.
#[test]
fn sigint_leaves_no_working_directory() {
    let run = interrupt_while_the_server_runs(Signal::SIGINT);

    assert_eq!(
        working_directories(&run.environment.servers()),
        Vec::<String>::new()
    );
}

/// int-1: SIGTERM exits with status 143.
#[test]
fn sigterm_exits_with_status_143() {
    let run = interrupt_while_the_server_runs(Signal::SIGTERM);

    assert_eq!(run.output.exit_code, 143);
}

/// int-1, d-new-3: on SIGTERM the server has exited by the time mism exits.
#[test]
fn sigterm_stops_the_server() {
    let run = interrupt_while_the_server_runs(Signal::SIGTERM);

    assert!(!fake_java::is_alive(run.server_pid));
}

/// int-1: on SIGTERM nothing is left at the path.
#[test]
fn sigterm_leaves_nothing_at_the_path() {
    let run = interrupt_while_the_server_runs(Signal::SIGTERM);

    assert!(nothing_exists_at(&run.environment.server_path()));
}

/// int-1 (「mism new の手順と中断」 方法: the working directory is removed after the server exits):
/// on SIGTERM no working directory is left beside the path.
#[test]
fn sigterm_leaves_no_working_directory() {
    let run = interrupt_while_the_server_runs(Signal::SIGTERM);

    assert_eq!(
        working_directories(&run.environment.servers()),
        Vec::<String>::new()
    );
}

/// The paths of the requests the fake PaperMC received for its API, which does not serve the jar.
fn api_request_paths(environment: &NewServerEnvironment) -> Vec<String> {
    environment
        .papermc
        .request_paths()
        .into_iter()
        .filter(|path| path.starts_with("/v3/projects/"))
        .collect()
}

/// The entry of `minecraft_version` in the saved Paper version list.
fn saved_paper_entry(minecraft_version: &str) -> Value {
    fake_papermc::saved_version_list("paper")["versions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["version"]["id"] == minecraft_version)
        .unwrap()
        .clone()
}

/// Runs `mism new paper <path>` with the `WaitsForRelease` fake `java`, and creates an empty
/// directory at the path while the server runs, after mism has checked the path.
fn create_while_the_path_appears() -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::WaitsForRelease);
    let mism = spawn_new_paper(&environment);
    environment.fake_java.wait_for_server_pid();
    fs::create_dir(environment.server_path()).unwrap();
    environment.fake_java.release_server();
    let output = mism.wait();
    (environment, output)
}

/// `PATH` is only an empty directory. It cannot include `/usr/bin`, where a real `java` may be.
fn create_without_java_on_path() -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let directory_without_java = environment.home().join("bin");
    fs::create_dir(&directory_without_java).unwrap();
    let output = environment
        .mism_new([
            OsString::from("paper"),
            environment.server_path().into_os_string(),
        ])
        .environment("PATH", directory_without_java)
        .run();
    (environment, output)
}

/// Java 21 is found, and 26.3 requires Java 25.
fn create_with_old_java() -> (NewServerEnvironment, Output) {
    create_paper_server(Behavior::OldVersion)
}

/// The jar's response starts 3 seconds after its request, and the idle timeout is 0.3 seconds.
/// Had mism waited for the response, it would have succeeded.
fn create_with_a_stalled_jar_download() -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("paper");
    environment.papermc.mount_jar_download_after(
        "paper",
        "26.3",
        206,
        JAR_BYTES,
        &fake_papermc::sha256_hex(JAR_BYTES),
        Duration::from_secs(3),
    );
    let output = environment
        .mism_new([
            OsString::from("paper"),
            environment.server_path().into_os_string(),
        ])
        .environment(IDLE_TIMEOUT_VARIABLE, "0.3")
        .run();
    (environment, output)
}

/// The jar arrives in 8 pieces 100 ms apart, and the idle timeout is 0.5 seconds: the download
/// takes longer than the idle timeout, but data never stops arriving for that long.
fn create_with_a_slow_jar_download() -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("paper");
    let jar_server = SlowJarServer::start(JAR_BYTES, 8, Duration::from_millis(100));
    environment.papermc.mount_download_information(
        "paper",
        "26.3",
        206,
        JAR_BYTES,
        &fake_papermc::sha256_hex(JAR_BYTES),
        jar_server.url(),
    );
    let output = environment
        .mism_new([
            OsString::from("paper"),
            environment.server_path().into_os_string(),
        ])
        .environment(IDLE_TIMEOUT_VARIABLE, "0.5")
        .run();
    (environment, output)
}

/// d-api-1: the version list and the download information; the jar is served from another path.
#[test]
fn successful_creation_calls_the_api_twice() {
    let (environment, _output) = create_paper_server(Behavior::Normal);

    assert_eq!(api_request_paths(&environment).len(), 2);
}

/// d-api-2: `26.03`, which the list does not contain, is put in no URL.
#[test]
fn requested_version_missing_from_the_list_is_put_in_no_url() {
    let (environment, _output) = create_requesting("26.03");

    assert!(
        !environment
            .papermc
            .request_paths()
            .iter()
            .any(|path| path.contains("26.03"))
    );
}

/// d-api-2: `26.3`, which the list contains, is put in the URL of the download information.
#[test]
fn requested_version_from_the_list_is_put_in_the_download_information_url() {
    let (environment, _output) = create_requesting("26.3");

    assert!(
        environment
            .papermc
            .request_paths()
            .iter()
            .any(|path| path == "/v3/projects/paper/versions/26.3/builds/206")
    );
}

/// d-api-3: the newest release, 26.3, is listed neither first nor last of the SUPPORTED entries,
/// and 26.3-rc-3 (made SUPPORTED) sorts after it as text, yet 26.3 is chosen.
#[test]
fn omitted_version_follows_release_order_over_list_order() {
    let mut release_candidate = saved_paper_entry("26.3-rc-3");
    release_candidate["version"]["support"]["status"] = "SUPPORTED".into();
    let version_list = json!({
        "versions": [
            saved_paper_entry("26.2"),
            saved_paper_entry("26.3"),
            release_candidate,
            saved_paper_entry("26.1.2"),
        ]
    });
    let environment = environment_serving(&version_list.to_string(), "26.3", 206);

    let output = run_new(&environment, "paper", &[]);

    assert_eq!(printed_fields(&output)[1], "26.3");
}

/// d-new-1: nothing exists at the path while the server runs, nor after mism is interrupted then.
#[test]
fn nothing_exists_at_the_path_before_the_preparation_is_complete() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Hangs);
    let mism = spawn_new_paper(&environment);
    environment.fake_java.wait_for_server_pid();

    let while_the_server_runs = nothing_exists_at(&environment.server_path());
    mism.send(Signal::SIGINT);
    mism.wait();
    let after_the_interruption = nothing_exists_at(&environment.server_path());

    assert_eq!(
        (while_the_server_runs, after_the_interruption),
        (true, true)
    );
}

/// d-new-2: exit status
#[test]
fn path_that_appears_during_the_preparation_exits_with_status_1() {
    let (_environment, output) = create_while_the_path_appears();

    assert_eq!(output.exit_code, 1);
}

/// d-new-2: the rename fails, and the error names the path that appeared.
#[test]
fn path_that_appears_during_the_preparation_is_an_error() {
    let (environment, output) = create_while_the_path_appears();

    assert_eq!(
        error_line(&output),
        format!(
            "mism: {} appeared while the server directory was being prepared",
            environment.server_path().display()
        )
    );
}

/// d-new-2: the working directory is removed.
#[test]
fn path_that_appears_during_the_preparation_leaves_no_working_directory() {
    let (environment, _output) = create_while_the_path_appears();

    assert_eq!(
        working_directories(&environment.servers()),
        Vec::<String>::new()
    );
}

/// d-new-4: SIGKILL while the server runs leaves nothing at the path.
#[test]
fn nothing_exists_at_the_path_after_mism_is_killed() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Hangs);
    let mism = spawn_new_paper(&environment);
    let server_pid = environment.fake_java.wait_for_server_pid();

    mism.kill();
    // In its own process group, the server outlives mism.
    kill(server_pid, Signal::SIGKILL).unwrap();

    assert!(nothing_exists_at(&environment.server_path()));
}

/// d-new-5: a communication failure leaves no working directory beside the path.
#[test]
fn unreachable_papermc_leaves_no_working_directory() {
    let (environment, _output, _base_url) = create_against_unreachable_papermc();

    assert_eq!(
        working_directories(&environment.servers()),
        Vec::<String>::new()
    );
}

/// d-launch-1: exit status
#[test]
fn missing_java_exits_with_status_1() {
    let (_environment, output) = create_without_java_on_path();

    assert_eq!(output.exit_code, 1);
}

/// d-launch-1: the error says that `java` was not found.
#[test]
fn missing_java_is_an_error() {
    let (_environment, output) = create_without_java_on_path();

    assert_eq!(error_line(&output), "mism: `java` was not found on PATH");
}

/// d-launch-1: no server is started.
#[test]
fn missing_java_starts_no_server() {
    let (environment, _output) = create_without_java_on_path();

    assert_eq!(environment.fake_java.invocation_count(), 0);
}

/// d-launch-2: exit status
#[test]
fn old_java_exits_with_status_1() {
    let (_environment, output) = create_with_old_java();

    assert_eq!(output.exit_code, 1);
}

/// d-launch-2: the error names the found and the required Java versions.
#[test]
fn old_java_is_an_error() {
    let (environment, output) = create_with_old_java();

    assert_eq!(
        error_line(&output),
        format!(
            "mism: {} is Java 21, older than the required Java 25",
            environment.fake_java.directory().join("java").display()
        )
    );
}

/// d-launch-2: `java -version` runs, but no server is started.
#[test]
fn old_java_starts_no_server() {
    let (environment, _output) = create_with_old_java();

    assert!(
        !environment
            .fake_java
            .invocations()
            .iter()
            .any(|invocation| invocation.starts_with("-jar"))
    );
}

/// d-http-1: exit status. The delayed response would have let mism succeed, so status 1 shows
/// that mism stopped waiting before it arrived.
#[test]
fn stalled_jar_download_exits_with_status_1() {
    let (_environment, output) = create_with_a_stalled_jar_download();

    assert_eq!(output.exit_code, 1);
}

/// d-http-1: the cause is that no data arrived for the idle timeout.
#[test]
fn stalled_jar_download_reports_that_no_data_arrived() {
    let (_environment, output) = create_with_a_stalled_jar_download();

    assert!(error_report_has_line(
        &output,
        "mism: caused by: io: no data arrived for 0.3 seconds"
    ));
}

/// d-http-2: exit status
#[test]
fn slow_jar_download_exits_with_status_0() {
    let (_environment, output) = create_with_a_slow_jar_download();

    assert_eq!(output.exit_code, 0);
}

/// d-http-2: the whole jar arrives.
#[test]
fn slow_jar_download_delivers_the_whole_jar() {
    let (environment, _output) = create_with_a_slow_jar_download();

    assert_eq!(
        fs::read(environment.server_path().join(JAR_NAME)).unwrap(),
        JAR_BYTES
    );
}

/// d-cli-1: the error output of `mism new` starts with `mism: `.
#[test]
fn error_output_starts_with_mism_prefix() {
    let (_environment, output, _base_url) = create_against_unreachable_papermc();

    assert!(output.stdout.starts_with("mism: "));
}

/// d-cli-2: the chain is the failed request (`http_client::Error::Request`, wrapped by the
/// transparent errors of `main`, `subcommands::new`, and `papermc_api`) and its cause, ureq's
/// error, which has no source of its own.
#[test]
fn each_cause_of_the_error_has_a_caused_by_line() {
    let (_environment, output, _base_url) = create_against_unreachable_papermc();

    let depth = output
        .stdout
        .lines()
        .filter(|line| line.starts_with("mism: caused by: "))
        .count();
    assert_eq!(depth, 1);
}
