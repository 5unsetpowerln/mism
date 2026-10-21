mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::datapack_server_directory::{DatapackEnvironment, WorldState};
use common::fake_modrinth::sha256_hex;
use common::management_environment::{Record, mism_toml_with_records};
use common::mism_process::Output;
use common::plugin_output::reported_an_error;

fn record_a(sha256: &str) -> Record<'_> {
    Record {
        name: "A",
        source: "../outside/A.zip",
        file_name: "A.zip",
        sha256,
    }
}

fn record_b(sha256: &str) -> Record<'_> {
    Record {
        name: "B",
        source: "https://example.com/B.zip",
        file_name: "B.zip",
        sha256,
    }
}

/// A plugin record, which stays whatever happens to the datapack records.
fn plugin_record(sha256: &str) -> Record<'_> {
    Record {
        name: "A",
        source: "../outside/A.jar",
        file_name: "A.jar",
        sha256,
    }
}

/// The text of the `mism.toml` with the plugin record and `datapacks`.
fn mism_toml_with_datapacks(datapacks: &[Record]) -> String {
    mism_toml_with_records(
        "paper",
        "26.3",
        &[plugin_record(&sha256_hex(b"plugin"))],
        datapacks,
    )
}

/// The parsed text of a `mism.toml`, to compare what it says rather than how it is formatted.
fn parsed(text: &str) -> toml::Table {
    text.parse().unwrap()
}

/// A Paper 26.3 server with the records of the datapacks `A` and `B`, their files in
/// `world/datapacks/`, a plugin record named `A`, and a file of the world.
fn server_with_two_datapacks() -> DatapackEnvironment {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    environment.write_mism_toml(&mism_toml_with_datapacks(&[
        record_a(&sha256_hex(b"a")),
        record_b(&sha256_hex(b"b")),
    ]));
    environment.put_in_datapacks("A.zip", b"a");
    environment.put_in_datapacks("B.zip", b"b");
    fs::write(environment.world().join("level.dat"), b"level").unwrap();
    environment
}

fn remove(environment: &DatapackEnvironment, name: &str) -> Output {
    environment.mism_datapack(["remove", name]).run()
}

/// drm-1: the recorded file is removed from `datapacks/`.
#[test]
fn recorded_file_is_removed() {
    let environment = server_with_two_datapacks();

    remove(&environment, "A");

    assert!(fs::symlink_metadata(environment.datapacks().join("A.zip")).is_err());
}

/// drm-1: the record is removed from `mism.toml`, and the other records stay.
#[test]
fn record_is_removed() {
    let environment = server_with_two_datapacks();

    remove(&environment, "A");

    assert_eq!(
        parsed(&environment.read_mism_toml()),
        parsed(&mism_toml_with_datapacks(&[record_b(&sha256_hex(b"b"))]))
    );
}

/// drm-1: the files and directories of the world directory other than the datapack file stay.
#[test]
fn other_files_of_the_world_stay() {
    let environment = server_with_two_datapacks();
    fs::create_dir_all(environment.world().join("region")).unwrap();
    fs::write(environment.world().join("region/r.0.0.mca"), b"region").unwrap();
    environment.put_in_datapacks("Unmanaged.zip", b"unmanaged");

    remove(&environment, "A");

    assert_eq!(
        [
            fs::read(environment.world().join("level.dat")).ok(),
            fs::read(environment.world().join("region/r.0.0.mca")).ok(),
            fs::read(environment.datapacks().join("Unmanaged.zip")).ok(),
            fs::read(environment.datapacks().join("B.zip")).ok(),
        ],
        [
            Some(b"level".to_vec()),
            Some(b"region".to_vec()),
            Some(b"unmanaged".to_vec()),
            Some(b"b".to_vec()),
        ]
    );
}

/// drm-1, dp-10: the removal writes only the notice.
#[test]
fn removal_writes_the_notice() {
    let environment = server_with_two_datapacks();

    let output = remove(&environment, "A");

    assert_eq!(
        output.stdout,
        "mism: the change takes effect when the server starts next\n"
    );
}

/// drm-1: the removal exits with status 0.
#[test]
fn removal_exits_with_status_0() {
    let environment = server_with_two_datapacks();

    let output = remove(&environment, "A");

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// drm-2: a name without a record is an error.
#[test]
fn unrecorded_name_is_an_error() {
    let environment = server_with_two_datapacks();

    let output = remove(&environment, "C");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// drm-2: a name without a record changes nothing.
#[test]
fn unrecorded_name_changes_nothing() {
    let environment = server_with_two_datapacks();
    let before = environment.world_state();

    remove(&environment, "C");

    assert_eq!(environment.world_state(), before);
}

/// drm-2: the name of a plugin record alone is a name without a datapack record.
#[test]
fn name_of_a_plugin_record_alone_is_an_error() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    environment.write_mism_toml(&mism_toml_with_datapacks(&[]));

    let output = remove(&environment, "A");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// The server with two datapacks, without the file of `A`.
fn server_without_the_file_of_a() -> DatapackEnvironment {
    let environment = server_with_two_datapacks();
    fs::remove_file(environment.datapacks().join("A.zip")).unwrap();
    environment
}

/// drm-3: a record whose file is not in `datapacks/` is removed with a message that names the
/// absolute path of `datapacks/`.
#[test]
fn record_without_its_file_is_removed_with_a_message() {
    let environment = server_without_the_file_of_a();

    let output = remove(&environment, "A");

    assert_eq!(
        output.stdout,
        format!(
            "mism: {}/A.zip does not exist, so only the record of A was removed\n",
            environment.datapacks().display()
        )
    );
}

/// drm-3: only the record is removed.
#[test]
fn record_without_its_file_is_removed() {
    let environment = server_without_the_file_of_a();

    remove(&environment, "A");

    assert_eq!(
        parsed(&environment.read_mism_toml()),
        parsed(&mism_toml_with_datapacks(&[record_b(&sha256_hex(b"b"))]))
    );
}

/// drm-3: the removal of the record exits with status 0.
#[test]
fn record_without_its_file_exits_with_status_0() {
    let environment = server_without_the_file_of_a();

    let output = remove(&environment, "A");

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// Runs `mism datapack remove A` with `datapacks/` read-only, so that the file cannot be
/// renamed, and returns the output and the state before the run.
fn remove_from_read_only_datapacks() -> (DatapackEnvironment, Output, WorldState) {
    let environment = server_with_two_datapacks();
    let before = environment.world_state();
    fs::set_permissions(environment.datapacks(), fs::Permissions::from_mode(0o555)).unwrap();

    let output = remove(&environment, "A");

    fs::set_permissions(environment.datapacks(), fs::Permissions::from_mode(0o755)).unwrap();
    (environment, output, before)
}

/// dp-11: when the file cannot be renamed in `datapacks/`, the world directory and `mism.toml`
/// are as they were.
#[test]
fn failure_to_rename_the_file_leaves_the_world_as_it_was() {
    let (environment, _output, before) = remove_from_read_only_datapacks();

    assert_eq!(environment.world_state(), before);
}

/// dp-11: the failure to rename the file is an error.
#[test]
fn failure_to_rename_the_file_is_an_error() {
    let (_environment, output, _before) = remove_from_read_only_datapacks();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}
