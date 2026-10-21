mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::fake_modrinth::sha256_hex;
use common::mism_process::Output;
use common::plugin_output::has_line;
use common::plugin_server_directory::{PluginEnvironment, Record, mism_toml_text};

/// A Paper 26.3 server with the records of `A` and `B` and their jars in `plugins/`.
fn server_with_two_plugins() -> PluginEnvironment {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let sha256_a = sha256_hex(b"a");
    let sha256_b = sha256_hex(b"b");
    environment.write_mism_toml(&mism_toml_text(
        "paper",
        "26.3",
        &[record_a(&sha256_a), record_b(&sha256_b)],
    ));
    environment.put_in_plugins("A.jar", b"a");
    environment.put_in_plugins("B.jar", b"b");
    environment
}

fn record_a(sha256: &str) -> Record<'_> {
    Record {
        name: "A",
        source: "../outside/A.jar",
        file_name: "A.jar",
        sha256,
    }
}

fn record_b(sha256: &str) -> Record<'_> {
    Record {
        name: "B",
        source: "https://example.com/B.jar",
        file_name: "B.jar",
        sha256,
    }
}

fn remove(environment: &PluginEnvironment, name: &str) -> Output {
    environment.mism_plugin(["remove", name]).run()
}

/// rm-1: the recorded jar is removed from `plugins/`.
#[test]
fn recorded_jar_is_removed() {
    let environment = server_with_two_plugins();

    remove(&environment, "A");

    assert!(!environment.plugins().join("A.jar").exists());
}

/// rm-1: the record is removed from `mism.toml`, and the other records stay.
#[test]
fn record_is_removed() {
    let environment = server_with_two_plugins();

    remove(&environment, "A");

    assert_eq!(
        environment.read_mism_toml(),
        mism_toml_text("paper", "26.3", &[record_b(&sha256_hex(b"b"))])
    );
}

/// rm-1: what the plugin created in `plugins/` stays.
#[test]
fn files_the_plugin_created_stay() {
    let environment = server_with_two_plugins();
    fs::create_dir(environment.plugins().join("A")).unwrap();
    fs::write(environment.plugins().join("A/config.yml"), "key: value\n").unwrap();

    remove(&environment, "A");

    assert_eq!(
        fs::read_to_string(environment.plugins().join("A/config.yml")).unwrap(),
        "key: value\n"
    );
}

/// rm-1, plg-7: the removal writes only the notice.
#[test]
fn removal_writes_the_notice() {
    let environment = server_with_two_plugins();

    let output = remove(&environment, "A");

    assert_eq!(
        output.stdout,
        "mism: the change takes effect when the server starts next\n"
    );
}

/// rm-1: the removal exits with status 0.
#[test]
fn removal_exits_with_status_0() {
    let environment = server_with_two_plugins();

    let output = remove(&environment, "A");

    assert_eq!(output.exit_code, 0);
}

/// rm-2: a name without a record is an error.
#[test]
fn unrecorded_name_is_an_error() {
    let environment = server_with_two_plugins();

    let output = remove(&environment, "C");

    assert!(has_line(
        &output.stdout,
        "mism.toml has no record of the plugin C"
    ));
}

/// rm-2: the error tells how to see the recorded names.
#[test]
fn unrecorded_name_hints_at_listing() {
    let environment = server_with_two_plugins();

    let output = remove(&environment, "C");

    assert!(has_line(
        &output.stdout,
        "hint: run `mism plugin list` to see the names in the records"
    ));
}

/// rm-2: a name without a record changes nothing.
#[test]
fn unrecorded_name_changes_nothing() {
    let environment = server_with_two_plugins();
    let before = environment.server_state();

    remove(&environment, "C");

    assert_eq!(environment.server_state(), before);
}

/// rm-3: a record whose jar is not in `plugins/` is removed with a message.
#[test]
fn record_without_its_jar_is_removed_with_a_message() {
    let environment = server_with_two_plugins();
    fs::remove_file(environment.plugins().join("A.jar")).unwrap();

    let output = remove(&environment, "A");

    assert_eq!(
        output.stdout,
        "mism: plugins/A.jar does not exist, so only the record of A was removed\n"
    );
}

/// rm-3: only the record is removed.
#[test]
fn record_without_its_jar_is_removed() {
    let environment = server_with_two_plugins();
    fs::remove_file(environment.plugins().join("A.jar")).unwrap();

    remove(&environment, "A");

    assert_eq!(
        environment.read_mism_toml(),
        mism_toml_text("paper", "26.3", &[record_b(&sha256_hex(b"b"))])
    );
}

/// rm-3: the removal of the record exits with status 0.
#[test]
fn record_without_its_jar_exits_with_status_0() {
    let environment = server_with_two_plugins();
    fs::remove_file(environment.plugins().join("A.jar")).unwrap();

    let output = remove(&environment, "A");

    assert_eq!(output.exit_code, 0);
}

/// A server whose record of `A` has the jar file name `../server.properties`.
fn server_with_a_record_outside_plugins() -> PluginEnvironment {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.write_mism_toml(&mism_toml_text(
        "paper",
        "26.3",
        &[Record {
            name: "A",
            source: "../outside/A.jar",
            file_name: "../server.properties",
            sha256: &sha256_hex(b"a"),
        }],
    ));
    fs::create_dir(environment.plugins()).unwrap();
    fs::write(environment.server().join("server.properties"), "motd=A\n").unwrap();
    environment
}

/// d-name-1: the file name `../server.properties` is an error.
#[test]
fn record_file_name_outside_plugins_is_an_error() {
    let environment = server_with_a_record_outside_plugins();

    let output = remove(&environment, "A");

    assert!(has_line(
        &output.stdout,
        "the file name in the record of A cannot be used"
    ));
}

/// d-name-1: nothing is removed.
#[test]
fn record_file_name_outside_plugins_removes_nothing() {
    let environment = server_with_a_record_outside_plugins();
    let before = (
        environment.server_state(),
        fs::read_to_string(environment.server().join("server.properties")).unwrap(),
    );

    remove(&environment, "A");

    assert_eq!(
        (
            environment.server_state(),
            fs::read_to_string(environment.server().join("server.properties")).unwrap(),
        ),
        before
    );
}

/// plg-8: when the jar cannot be renamed in `plugins/`, `plugins/` and `mism.toml` are as they
/// were.
#[test]
fn failure_to_rename_the_jar_leaves_the_server_as_it_was() {
    let environment = server_with_two_plugins();
    let before = environment.server_state();
    fs::set_permissions(environment.plugins(), fs::Permissions::from_mode(0o555)).unwrap();

    remove(&environment, "A");

    fs::set_permissions(environment.plugins(), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(environment.server_state(), before);
}
