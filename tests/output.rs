mod common;

use std::ffi::OsString;

use common::fake_java::{self, Behavior};
use common::fake_papermc::{self, FakePaperMc};
use common::mism_process::{MismProcess, Output};
use common::new_server_environment::NewServerEnvironment;
use common::pseudo_terminal;
use common::server_output::{end_line, start_line};
use common::temporary_directory;

const JAR_NAME: &str = "paper-26.3-206.jar";
/// `mism` in SGR 34 (blue), followed by SGR 0 (reset).
const BLUE_MISM: &str = "\x1b[34mmism\x1b[0m";

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

/// Runs mism like `create_paper_server`, but with its stderr on a pseudo-terminal, and returns
/// what mism and the server wrote to that terminal.
fn create_paper_server_with_stderr_on_a_terminal() -> String {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let terminal = pseudo_terminal::open();
    let mism = environment
        .mism_new(paper_arguments(&environment))
        .stderr(terminal.slave)
        .spawn();
    let stderr = pseudo_terminal::read_until_closed(terminal.master);
    mism.wait();
    stderr
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

/// `stderr` without the empty lines and the server's output between the start line and the end
/// line (out-2).
fn without_server_output(stderr: &str) -> String {
    let start = format!("{}\n", start_line(JAR_NAME));
    let end = format!("{}\n", end_line(JAR_NAME));
    let (before_start, after_start) = stderr
        .split_once(&start)
        .unwrap_or_else(|| panic!("stderr has no start line: {stderr}"));
    let (_, after_end) = after_start
        .split_once(&end)
        .unwrap_or_else(|| panic!("stderr has no end line: {stderr}"));
    format!("{before_start}{start}{end}{after_end}")
}

/// out-1: stdout of a successful `mism new`
#[test]
fn every_line_on_stdout_of_mism_new_starts_with_the_prefix() {
    let output = create_paper_server();

    assert_eq!(lines_without_prefix(&output.stdout), Vec::<&str>::new());
}

/// out-1: stderr of a successful `mism new`, apart from the empty lines and the server's output
/// of out-2
#[test]
fn every_line_mism_writes_on_stderr_of_mism_new_starts_with_the_prefix() {
    let output = create_paper_server();

    assert_eq!(
        lines_without_prefix(&without_server_output(&output.stderr)),
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

    assert_eq!(lines_without_prefix(&output.stderr), Vec::<&str>::new());
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
        output.stderr.contains(&expected),
        "stderr: {}",
        output.stderr
    );
}

/// out-3: stderr on a terminal. Only the leading `mism` of the start line and of the end line is
/// colored, in blue.
#[test]
fn on_a_terminal_the_leading_mism_of_the_start_and_end_lines_is_blue() {
    let stderr = create_paper_server_with_stderr_on_a_terminal();

    let lines_of_out_2: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("output of "))
        .collect();
    assert_eq!(
        lines_of_out_2,
        [
            format!("{BLUE_MISM}: output of {JAR_NAME} follows"),
            format!("{BLUE_MISM}: output of {JAR_NAME} ended"),
        ]
    );
}

/// out-3: stderr that is not a terminal
#[test]
fn on_a_pipe_stderr_has_no_escape_sequence() {
    let output = create_paper_server();

    assert!(
        !output.stderr.contains('\x1b'),
        "stderr: {:?}",
        output.stderr
    );
}
