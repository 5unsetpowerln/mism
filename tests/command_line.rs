mod common;

use common::mism_process::{MismProcess, Output};

fn run(arguments: &[&str]) -> Output {
    MismProcess::new().arguments(arguments).run()
}

/// cli-1: stdout
#[test]
fn help_prints_usage_to_stdout() {
    assert!(run(&["--help"]).stdout.contains("Usage:"));
}

/// cli-1: exit status
#[test]
fn help_exits_with_zero() {
    assert_eq!(run(&["--help"]).exit_code, 0);
}

/// cli-2: stdout
#[test]
fn version_prints_own_version_to_stdout() {
    assert_eq!(
        run(&["--version"]).stdout.trim(),
        concat!("mism ", env!("CARGO_PKG_VERSION"))
    );
}

/// cli-2: exit status
#[test]
fn version_exits_with_zero() {
    assert_eq!(run(&["--version"]).exit_code, 0);
}

/// cli-3: `mism new --help` stdout
#[test]
fn new_help_prints_usage_to_stdout() {
    assert!(run(&["new", "--help"]).stdout.contains("Usage: mism new"));
}

/// cli-3: `mism new --help` exit status
#[test]
fn new_help_exits_with_zero() {
    assert_eq!(run(&["new", "--help"]).exit_code, 0);
}

/// cli-3: `mism versions --help` stdout
#[test]
fn versions_help_prints_usage_to_stdout() {
    assert!(
        run(&["versions", "--help"])
            .stdout
            .contains("Usage: mism versions")
    );
}

/// cli-3: `mism versions --help` exit status
#[test]
fn versions_help_exits_with_zero() {
    assert_eq!(run(&["versions", "--help"]).exit_code, 0);
}

/// cli-4: usage text on stderr
#[test]
fn invalid_arguments_print_usage_to_stderr() {
    assert!(run(&["no-such-subcommand"]).stderr.contains("Usage:"));
}

/// cli-4: stdout stays empty
#[test]
fn invalid_arguments_print_nothing_to_stdout() {
    assert_eq!(run(&["no-such-subcommand"]).stdout, "");
}

/// cli-4: exit status
#[test]
fn invalid_arguments_exit_with_two() {
    assert_eq!(run(&["no-such-subcommand"]).exit_code, 2);
}

/// cli-5: `mism new` usage text on stderr
#[test]
fn new_unknown_software_prints_usage_to_stderr() {
    assert!(
        run(&["new", "spigot", "dir"])
            .stderr
            .contains("Usage: mism new")
    );
}

/// cli-5: `mism versions` usage text on stderr
#[test]
fn versions_unknown_software_prints_usage_to_stderr() {
    assert!(
        run(&["versions", "spigot"])
            .stderr
            .contains("Usage: mism versions")
    );
}

/// cli-5: exit status
#[test]
fn unknown_software_exits_with_two() {
    assert_eq!(run(&["new", "spigot", "dir"]).exit_code, 2);
}
