mod common;

use std::ffi::OsString;

use common::datapack_server_directory::DatapackEnvironment;
use common::fake_java::{self, Behavior};
use common::fake_papermc::{self, FakePaperMc};
use common::mism_process::{MismProcess, Output};
use common::new_server_environment::NewServerEnvironment;
use common::pseudo_terminal;
use common::server_output::{end_line, start_line};
use common::temporary_directory;
use common::test_datapack_zip::datapack_declaring;

const JAR_NAME: &str = "paper-26.3-206.jar";
/// `mism:` in SGR 34 (blue), followed by SGR 0 (reset).
const BLUE_MISM: &str = "\x1b[34mmism:\x1b[0m";
/// `mism:` in SGR 33 (yellow), followed by SGR 0 (reset).
const YELLOW_MISM: &str = "\x1b[33mmism:\x1b[0m";
/// The warning of dinsp-6 for a datapack of the format 48 on a server of the format 121.0, after
/// `mism: ` or the yellow `mism:`.
const FORMAT_WARNING: &str = "warning: the datapack declares the data pack formats 48 to 48, which do not include 121.0 of \
     the server";

fn paper_arguments(environment: &NewServerEnvironment) -> [OsString; 2] {
    [
        OsString::from("paper"),
        environment.server_path().into_os_string(),
    ]
}

/// Runs `mism new paper <path>` against the saved Paper version list with the `Normal` fake
/// `java`.
fn create_paper_server() -> Output {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    environment.mism_new(paper_arguments(&environment)).run()
}

/// Runs mism like `create_paper_server`, but with its stdout on a pseudo-terminal, and returns
/// what mism and the server wrote to that terminal.
fn create_paper_server_with_stdout_on_a_terminal() -> String {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let terminal = pseudo_terminal::open();
    let mism = environment
        .mism_new(paper_arguments(&environment))
        .stdout(terminal.slave)
        .spawn();
    let stdout = pseudo_terminal::read_until_closed(terminal.master);
    mism.wait();
    stdout
}

/// Runs `mism versions paper` against the PaperMC API at `papermc_url`.
fn list_paper_versions(papermc_url: &str) -> Output {
    let home = temporary_directory::create();
    MismProcess::new()
        .arguments(["versions", "paper"])
        .environment("HOME", home.path())
        .environment("MISM_PAPERMC_URL", papermc_url)
        .run()
}

fn lines_without_prefix(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.starts_with("mism: "))
        .collect()
}

/// `stdout` without the empty lines and the server's output between the start line and the end
/// line (out-2).
fn without_server_output(stdout: &str) -> String {
    let start = format!("{}\n", start_line(JAR_NAME));
    let end = format!("{}\n", end_line(JAR_NAME));
    let (before_start, after_start) = stdout
        .split_once(&start)
        .unwrap_or_else(|| panic!("stdout has no start line: {stdout}"));
    let (_, after_end) = after_start
        .split_once(&end)
        .unwrap_or_else(|| panic!("stdout has no end line: {stdout}"));
    format!("{before_start}{start}{end}{after_end}")
}

/// out-1: stdout of a successful `mism new`, apart from the empty lines and the server's output
/// of out-2
#[test]
fn every_line_on_stdout_of_mism_new_starts_with_the_prefix() {
    let output = create_paper_server();

    assert_eq!(
        lines_without_prefix(&without_server_output(&output.stdout)),
        Vec::<&str>::new()
    );
}

/// out-1: stdout of `mism versions`
#[test]
fn every_line_on_stdout_of_mism_versions_starts_with_the_prefix() {
    let papermc = FakePaperMc::start();
    papermc.mount_version_list_from_fixture("paper");

    let output = list_paper_versions(&papermc.base_url());

    assert_eq!(lines_without_prefix(&output.stdout), Vec::<&str>::new());
}

/// out-1: an error report with a cause and a hint (err-5)
#[test]
fn every_line_of_an_error_report_starts_with_the_prefix() {
    let output = list_paper_versions(&fake_papermc::unreachable_base_url());

    assert_eq!(lines_without_prefix(&output.stdout), Vec::<&str>::new());
}

/// out-2: the start line, an empty line, the server's lines unchanged, an empty line, and the end
/// line
#[test]
fn server_output_comes_between_the_start_line_and_the_end_line() {
    let output = create_paper_server();

    let expected = format!(
        "{}\n\n{}\n{}\n\n{}\n",
        start_line(JAR_NAME),
        fake_java::server_stdout_line(JAR_NAME),
        fake_java::SERVER_STDERR_LINE,
        end_line(JAR_NAME)
    );
    assert!(
        output.stdout.contains(&expected),
        "stdout: {}",
        output.stdout
    );
}

/// out-3: stdout on a terminal. The leading `mism:` of the start line and of the end line is
/// blue, and the rest of them is not colored.
#[test]
fn on_a_terminal_the_leading_mism_of_the_start_and_end_lines_is_blue() {
    let stdout = create_paper_server_with_stdout_on_a_terminal();

    let lines_of_out_2: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains("output of "))
        .collect();
    assert_eq!(
        lines_of_out_2,
        [
            format!("{BLUE_MISM} output of {JAR_NAME} follows"),
            format!("{BLUE_MISM} output of {JAR_NAME} ended"),
        ]
    );
}

/// out-3: stdout that is not a terminal
#[test]
fn on_a_pipe_stdout_has_no_escape_sequence() {
    let output = create_paper_server();

    assert!(
        !output.stdout.contains('\x1b'),
        "stdout: {:?}",
        output.stdout
    );
}

/// out-4: a successful `mism new`, whose server writes to its stderr too
#[test]
fn mism_new_writes_nothing_to_stderr() {
    let output = create_paper_server();

    assert_eq!(output.stderr, "");
}

/// out-4: `mism versions`
#[test]
fn mism_versions_writes_nothing_to_stderr() {
    let papermc = FakePaperMc::start();
    papermc.mount_version_list_from_fixture("paper");

    let output = list_paper_versions(&papermc.base_url());

    assert_eq!(output.stderr, "");
}

/// out-4: an error report with a cause and a hint (err-5)
#[test]
fn error_report_writes_nothing_to_stderr() {
    let output = list_paper_versions(&fake_papermc::unreachable_base_url());

    assert_eq!(output.stderr, "");
}

/// `mism datapack inspect` of a local datapack that declares the format 48, on a Paper 26.3
/// server of the format 121.0, which warns about `pack_format` (dinsp-6).
fn inspect_a_mismatching_datapack(environment: &DatapackEnvironment) -> MismProcess {
    let zip =
        environment.write_local_zip("Example.zip", &datapack_declaring(r#""pack_format": 48"#));
    environment.mism_datapack(["inspect", zip.to_str().unwrap()])
}

/// out-5: on a pipe, the warning line starts with `mism: warning: `.
#[test]
fn warning_line_starts_with_the_warning_prefix() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");

    let output = inspect_a_mismatching_datapack(&environment).run();

    let warnings: Vec<&str> = output
        .stdout
        .lines()
        .filter(|line| line.contains(FORMAT_WARNING))
        .collect();
    assert_eq!(warnings, [format!("mism: {FORMAT_WARNING}")]);
}

/// out-5: stdout on a terminal. Only the leading `mism:` of the warning line is colored, in
/// yellow.
#[test]
fn on_a_terminal_the_leading_mism_of_a_warning_is_yellow() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let terminal = pseudo_terminal::open();
    let mism = inspect_a_mismatching_datapack(&environment)
        .stdout(terminal.slave)
        .spawn();
    let stdout = pseudo_terminal::read_until_closed(terminal.master);
    mism.wait();

    let warnings: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains(FORMAT_WARNING))
        .collect();
    assert_eq!(warnings, [format!("{YELLOW_MISM} {FORMAT_WARNING}")]);
}

/// out-5: stdout that is not a terminal has no escape sequence in a warning either.
#[test]
fn on_a_pipe_a_warning_has_no_escape_sequence() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");

    let output = inspect_a_mismatching_datapack(&environment).run();

    assert!(
        !output.stdout.contains('\x1b'),
        "stdout: {:?}",
        output.stdout
    );
}

/// out-3: stdout on a terminal. The result line of `mism new` (new-10) starts with a blue
/// `mism:`.
#[test]
fn on_a_terminal_the_result_line_starts_with_a_blue_mism() {
    let stdout = create_paper_server_with_stdout_on_a_terminal();

    let last_line = stdout.lines().last().unwrap_or_default();
    assert!(
        last_line.starts_with(&format!("{BLUE_MISM} paper 26.3 206 ")),
        "stdout: {stdout:?}"
    );
}

/// out-3: stdout on a terminal. Every line of an inspection but the warning starts with a blue
/// `mism:`, and the rest of each line is not colored.
#[test]
fn on_a_terminal_each_line_but_a_warning_starts_with_a_blue_mism() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let terminal = pseudo_terminal::open();
    let mism = inspect_a_mismatching_datapack(&environment)
        .stdout(terminal.slave)
        .spawn();
    let stdout = pseudo_terminal::read_until_closed(terminal.master);
    mism.wait();

    let other_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.contains(FORMAT_WARNING))
        .filter(|line| {
            line.strip_prefix(&format!("{BLUE_MISM} "))
                .is_none_or(|rest| rest.contains('\x1b'))
        })
        .collect();
    assert_eq!(other_lines, Vec::<&str>::new(), "stdout: {stdout:?}");
}
