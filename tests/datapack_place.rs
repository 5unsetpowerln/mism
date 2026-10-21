mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

use common::datapack_server_directory::{DatapackEnvironment, WorldState};
use common::fake_modrinth::{
    ServedFile, TRUE_ENDING_FILE_NAME, TRUE_ENDING_NAME, TRUE_ENDING_SOURCE, sha256_hex,
};
use common::management_environment::{Record, mism_toml_with_records};
use common::mism_process::Output;
use common::plugin_output::{described, line_index, reported_an_error};
use common::test_datapack_zip::{
    datapack_declaring, datapack_with_pack_mcmeta, not_a_zip_file, true_ending_pack_mcmeta,
    zip_with_pack_mcmeta_in_a_directory,
};

const TAKES_EFFECT_NOTICE: &str = "the change takes effect when the server starts next";

/// A datapack for exactly the datapack format 121, that of the test servers.
fn example_datapack() -> Vec<u8> {
    datapack_declaring(r#""min_format": 121, "max_format": 121"#)
}

fn true_ending_datapack() -> Vec<u8> {
    datapack_with_pack_mcmeta(&true_ending_pack_mcmeta())
}

/// The parsed text of a `mism.toml`, to compare what it says rather than how it is formatted.
fn parsed(text: &str) -> toml::Table {
    text.parse().unwrap()
}

/// A Paper server for `minecraft_version`, with True Ending 1.1.4d fetched from the fake
/// Modrinth.
fn true_ending_fetched(minecraft_version: &str) -> DatapackEnvironment {
    let environment = DatapackEnvironment::with_server("paper", minecraft_version);
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());
    environment.fetch(TRUE_ENDING_SOURCE);
    environment
}

fn place_true_ending(environment: &DatapackEnvironment) -> Output {
    environment
        .mism_datapack(["place", TRUE_ENDING_SOURCE])
        .run()
}

/// `environment`'s local `Example.zip` with `bytes`, fetched; returns its path, which is its
/// source.
fn fetch_local_example(environment: &DatapackEnvironment, bytes: &[u8]) -> String {
    let zip = environment.write_local_zip("Example.zip", bytes);
    environment.fetch(zip.to_str().unwrap());
    zip.to_str().unwrap().to_owned()
}

/// Fetches the local `Example.zip` with `bytes` on `environment` prepared by `prepare`, and
/// places it. Returns the state of the world directory and `mism.toml` before the placement too.
fn place_example_in(
    environment: DatapackEnvironment,
    bytes: &[u8],
    prepare: impl FnOnce(&DatapackEnvironment),
) -> (DatapackEnvironment, Output, WorldState) {
    prepare(&environment);
    let source = fetch_local_example(&environment, bytes);
    let before = environment.world_state();
    let output = environment.mism_datapack(["place", &source]).run();
    (environment, output, before)
}

/// `place_example_in` on a Paper 26.3 server whose datapack format is 121.0.
fn place_example(
    bytes: &[u8],
    prepare: impl FnOnce(&DatapackEnvironment),
) -> (DatapackEnvironment, Output, WorldState) {
    place_example_in(
        DatapackEnvironment::with_server("paper", "26.3"),
        bytes,
        prepare,
    )
}

fn placed(environment: &DatapackEnvironment, file_name: &str) -> Option<Vec<u8>> {
    fs::read(environment.datapacks().join(file_name)).ok()
}

/// dplace-1: a source that was not fetched is an error.
#[test]
fn source_not_fetched_is_an_error() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");

    let output = place_true_ending(&environment);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-1: a source that was not fetched is not fetched by `place`.
#[test]
fn source_not_fetched_sends_no_request() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    place_true_ending(&environment);

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// dplace-1: a Modrinth source that is not the one the file was fetched with does not find it.
#[test]
fn other_modrinth_source_does_not_find_the_file() {
    let environment = true_ending_fetched("26.3");

    let output = environment
        .mism_datapack(["place", "modrinth:true-ending"])
        .run();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-1: a local datapack file finds the cached file with its SHA-256, however it was
/// fetched.
#[test]
fn local_file_finds_the_cached_file_with_its_sha256() {
    let environment = true_ending_fetched("26.3");
    let zip = environment.write_local_zip(TRUE_ENDING_FILE_NAME, &true_ending_datapack());

    let output = environment
        .mism_datapack(["place", zip.to_str().unwrap()])
        .run();

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dplace-1: placing sends no request.
#[test]
fn place_sends_no_request() {
    let environment = true_ending_fetched("26.3");
    let requests_before = environment.modrinth.request_count();

    place_true_ending(&environment);

    assert_eq!(environment.modrinth.request_count(), requests_before);
}

/// dplace-2: the file is directly in the world directory's `datapacks/`, which did not exist
/// with the world directory.
#[test]
fn file_is_placed_in_the_worlds_datapacks() {
    let environment = true_ending_fetched("26.3");

    place_true_ending(&environment);

    assert_eq!(
        placed(&environment, TRUE_ENDING_FILE_NAME),
        Some(true_ending_datapack())
    );
}

/// dplace-2: an existing world directory without `datapacks/` gets one.
#[test]
fn datapacks_is_created_in_an_existing_world() {
    let environment = true_ending_fetched("26.3");
    fs::create_dir(environment.world()).unwrap();
    fs::write(environment.world().join("level.dat"), b"level").unwrap();

    place_true_ending(&environment);

    assert_eq!(
        placed(&environment, TRUE_ENDING_FILE_NAME),
        Some(true_ending_datapack())
    );
}

/// dplace-2, dp-7, dp-6: `mism.toml` records the datapack name, the source, the file name, and
/// the SHA-256.
#[test]
fn placed_datapack_is_recorded() {
    let environment = true_ending_fetched("26.3");

    place_true_ending(&environment);

    assert_eq!(
        parsed(&environment.read_mism_toml()),
        parsed(&mism_toml_with_records(
            "paper",
            "26.3",
            &[],
            &[Record {
                name: TRUE_ENDING_NAME,
                source: TRUE_ENDING_SOURCE,
                file_name: TRUE_ENDING_FILE_NAME,
                sha256: &sha256_hex(&true_ending_datapack()),
            }]
        ))
    );
}

/// dp-7: a datapack from a local file is recorded with the path given as its source.
#[test]
fn local_datapack_is_recorded_with_its_path() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let source = fetch_local_example(&environment, &example_datapack());

    environment.mism_datapack(["place", &source]).run();

    assert_eq!(
        parsed(&environment.read_mism_toml()),
        parsed(&mism_toml_with_records(
            "paper",
            "26.3",
            &[],
            &[Record {
                name: "Example",
                source: &source,
                file_name: "Example.zip",
                sha256: &sha256_hex(&example_datapack()),
            }]
        ))
    );
}

/// dp-7: the datapack record is added after the records already there, and the plugin records
/// stay.
#[test]
fn record_is_added_to_the_records_there() {
    let plugin = Record {
        name: "LuckPerms",
        source: "modrinth:luckperms:v5.5.71-bukkit",
        file_name: "LuckPerms-Bukkit-5.5.71.jar",
        sha256: &sha256_hex(b"LuckPerms"),
    };
    let datapack = Record {
        name: "Other",
        source: "https://example.com/Other.zip",
        file_name: "Other.zip",
        sha256: &sha256_hex(b"Other"),
    };
    let environment = true_ending_fetched("26.3");
    environment.write_mism_toml(&mism_toml_with_records(
        "paper",
        "26.3",
        &[plugin],
        &[datapack],
    ));

    place_true_ending(&environment);

    let added = Record {
        name: TRUE_ENDING_NAME,
        source: TRUE_ENDING_SOURCE,
        file_name: TRUE_ENDING_FILE_NAME,
        sha256: &sha256_hex(&true_ending_datapack()),
    };
    assert_eq!(
        parsed(&environment.read_mism_toml()),
        parsed(&mism_toml_with_records(
            "paper",
            "26.3",
            &[plugin],
            &[datapack, added]
        ))
    );
}

/// `mism.toml` with a datapack record of `Example` whose file is `Old.zip`.
fn datapack_record_named_example(environment: &DatapackEnvironment) {
    environment.write_mism_toml(&mism_toml_with_records(
        "paper",
        "26.3",
        &[],
        &[Record {
            name: "Example",
            source: "https://example.com/Old.zip",
            file_name: "Old.zip",
            sha256: &sha256_hex(b"Old"),
        }],
    ));
}

/// dplace-3: a datapack record with the same name is an error.
#[test]
fn record_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        place_example(&example_datapack(), datapack_record_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-3: such a placement changes nothing.
#[test]
fn record_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        place_example(&example_datapack(), datapack_record_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dplace-3: a plugin record with the same name is not a datapack record.
#[test]
fn plugin_record_with_the_same_name_does_not_stop_the_placement() {
    let (_environment, output, _before) = place_example(&example_datapack(), |environment| {
        environment.write_mism_toml(&mism_toml_with_records(
            "paper",
            "26.3",
            &[Record {
                name: "Example",
                source: "https://example.com/Example.jar",
                file_name: "Example.jar",
                sha256: &sha256_hex(b"Example"),
            }],
            &[],
        ));
    });

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// Puts a file named `Example.zip`, of other content, in `datapacks/`.
fn put_file_named_example(environment: &DatapackEnvironment) {
    environment.put_in_datapacks("Example.zip", b"other");
}

/// Puts a directory named `Example.zip` in `datapacks/`.
fn put_directory_named_example(environment: &DatapackEnvironment) {
    fs::create_dir_all(environment.datapacks().join("Example.zip")).unwrap();
}

/// Puts a symbolic link named `Example.zip` without a target in `datapacks/`.
fn put_dangling_symlink_named_example(environment: &DatapackEnvironment) {
    fs::create_dir_all(environment.datapacks()).unwrap();
    symlink("no-such-file", environment.datapacks().join("Example.zip")).unwrap();
}

/// dplace-4: a file with the same name in `datapacks/` is an error.
#[test]
fn file_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        place_example(&example_datapack(), put_file_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-4: a file with the same name changes nothing.
#[test]
fn file_with_the_same_name_changes_nothing() {
    let (environment, _output, before) = place_example(&example_datapack(), put_file_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dplace-4: a directory with the same name in `datapacks/` is an error.
#[test]
fn directory_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        place_example(&example_datapack(), put_directory_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-4: a directory with the same name changes nothing.
#[test]
fn directory_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        place_example(&example_datapack(), put_directory_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dplace-4: a symbolic link with the same name in `datapacks/`, even one without a target, is
/// an error.
#[test]
fn dangling_symlink_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        place_example(&example_datapack(), put_dangling_symlink_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-4: a symbolic link with the same name changes nothing.
#[test]
fn dangling_symlink_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        place_example(&example_datapack(), put_dangling_symlink_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dplace-5, dp-10: the record of the placed datapack, then the notice, end the output.
#[test]
fn placement_ends_with_its_record_and_the_notice() {
    let environment = true_ending_fetched("26.3");

    let output = place_true_ending(&environment);

    assert!(
        output.stdout.ends_with(&format!(
            "\nmism: {TRUE_ENDING_NAME}\t1.1.4d\t{TRUE_ENDING_FILE_NAME}\n\
             mism: {TAKES_EFFECT_NOTICE}\n"
        )),
        "stdout: {}",
        output.stdout
    );
}

/// dplace-5: a datapack not from Modrinth has `-` as its version number.
#[test]
fn local_datapack_record_has_no_version_number() {
    let (_environment, output, _before) = place_example(&example_datapack(), |_| {});

    assert!(
        output.stdout.ends_with(&format!(
            "\nmism: Example\t-\tExample.zip\nmism: {TAKES_EFFECT_NOTICE}\n"
        )),
        "stdout: {}",
        output.stdout
    );
}

/// dplace-5: a successful placement exits with status 0.
#[test]
fn successful_placement_exits_with_status_0() {
    let environment = true_ending_fetched("26.3");

    let output = place_true_ending(&environment);

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dplace-6: the inspection of the file, with its judgements, precedes the record of the placed
/// datapack.
#[test]
fn inspection_precedes_the_record() {
    let environment = true_ending_fetched("26.3");

    let output = place_true_ending(&environment);

    let indices = [
        line_index(&output.stdout, &format!("name\t{TRUE_ENDING_NAME}")),
        line_index(&output.stdout, "declared\tmismatch"),
        line_index(&output.stdout, "pack_format\tmatch"),
        line_index(
            &output.stdout,
            &format!("{TRUE_ENDING_NAME}\t1.1.4d\t{TRUE_ENDING_FILE_NAME}"),
        ),
    ];
    assert!(
        indices.iter().all(Option::is_some) && indices.is_sorted(),
        "stdout: {}",
        output.stdout
    );
}

/// dplace-6: `declared` as `mismatch` does not stop the placement.
#[test]
fn declared_mismatch_does_not_stop_the_placement() {
    let environment = true_ending_fetched("26.3");

    place_true_ending(&environment);

    assert_eq!(
        placed(&environment, TRUE_ENDING_FILE_NAME),
        Some(true_ending_datapack())
    );
}

/// dplace-6 (「検証の値と判定」 方法): `pack_format` as `mismatch` does not stop the placement.
#[test]
fn pack_format_mismatch_does_not_stop_the_placement() {
    let bytes = datapack_declaring(r#""pack_format": 48"#);

    let (environment, _output, _before) = place_example(&bytes, |_| {});

    assert_eq!(placed(&environment, "Example.zip"), Some(bytes));
}

/// dplace-6: `pack_format` as `unknown` does not stop the placement.
#[test]
fn pack_format_unknown_does_not_stop_the_placement() {
    let bytes = datapack_declaring("");

    let (environment, _output, _before) = place_example(&bytes, |_| {});

    assert_eq!(placed(&environment, "Example.zip"), Some(bytes));
}

/// dplace-6: `missing` does not stop the placement.
#[test]
fn missing_does_not_stop_the_placement() {
    let environment = DatapackEnvironment::with_server("paper", "26.1.2");
    environment
        .modrinth
        .mount_project("stellarity_x_true_ending_patch");
    environment.modrinth.mount_version_list(
        "stellarity_x_true_ending_patch",
        &[ServedFile::new(
            "Stellarity_Mechanics_Patch.zip",
            &example_datapack(),
        )],
    );
    environment
        .modrinth
        .mount_projects("stellarity-true-ending", &["MCnBYP0b", "bZgeDzN8"]);
    let source = "modrinth:stellarity_x_true_ending_patch:1.0.0:Stellarity_Mechanics_Patch.zip";
    environment.fetch(source);

    environment.mism_datapack(["place", source]).run();

    assert_eq!(
        placed(&environment, "Stellarity_Mechanics_Patch.zip"),
        Some(example_datapack())
    );
}

/// dplace-6, dinsp-4: a file that is not a zip file is an error.
#[test]
fn file_that_is_not_zip_is_an_error() {
    let (_environment, output, _before) = place_example(&not_a_zip_file(), |_| {});

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-6, dinsp-4: a file that is not a zip file changes nothing.
#[test]
fn file_that_is_not_zip_changes_nothing() {
    let (environment, _output, before) = place_example(&not_a_zip_file(), |_| {});

    assert_eq!(environment.world_state(), before);
}

/// dplace-6 (「mism plugin place と mism datapack place と配置の手順」 方法 3): for a file that is
/// not a zip file, mism writes no inspection.
#[test]
fn file_that_is_not_zip_is_not_inspected() {
    let (_environment, output, _before) = place_example(&not_a_zip_file(), |_| {});

    assert_eq!(described(&output.stdout, "name"), None);
}

/// dplace-6, dinsp-4: a zip file without a top-level `pack.mcmeta` is an error.
#[test]
fn zip_without_a_top_level_pack_mcmeta_is_an_error() {
    let (_environment, output, _before) =
        place_example(&zip_with_pack_mcmeta_in_a_directory(), |_| {});

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-6, dinsp-4: a zip file without a top-level `pack.mcmeta` changes nothing.
#[test]
fn zip_without_a_top_level_pack_mcmeta_changes_nothing() {
    let (environment, _output, before) =
        place_example(&zip_with_pack_mcmeta_in_a_directory(), |_| {});

    assert_eq!(environment.world_state(), before);
}

/// dplace-6, dinsp-4: a declaration with `min_format` and without `max_format` is an error.
#[test]
fn min_format_alone_is_an_error() {
    let (_environment, output, _before) =
        place_example(&datapack_declaring(r#""min_format": 121"#), |_| {});

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-6, dinsp-4: a declaration with `min_format` and without `max_format` changes nothing.
#[test]
fn min_format_alone_changes_nothing() {
    let (environment, _output, before) =
        place_example(&datapack_declaring(r#""min_format": 121"#), |_| {});

    assert_eq!(environment.world_state(), before);
}

/// dplace-6, dinsp-5: a server without its server jar is an error.
#[test]
fn server_without_its_jar_is_an_error() {
    let (_environment, output, _before) = place_example_in(
        DatapackEnvironment::with_server_without_jar("paper", "26.3"),
        &example_datapack(),
        |_| {},
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dplace-6, dinsp-5: a server without its server jar changes nothing.
#[test]
fn server_without_its_jar_changes_nothing() {
    let (environment, _output, before) = place_example_in(
        DatapackEnvironment::with_server_without_jar("paper", "26.3"),
        &example_datapack(),
        |_| {},
    );

    assert_eq!(environment.world_state(), before);
}

/// d-dpdir-1
#[test]
fn nested_world_directory_is_created() {
    let (environment, _output, _before) = place_example_in(
        DatapackEnvironment::with_server("paper", "26.3").with_level_name("worlds/survival"),
        &example_datapack(),
        |_| {},
    );

    assert_eq!(
        fs::read(
            environment
                .server()
                .join("worlds/survival/datapacks/Example.zip")
        )
        .ok(),
        Some(example_datapack())
    );
}

/// Places the local `Example.zip` on a Paper 26.3 server whose `level-name` is the absolute path
/// `world`, with the server directory made read-only after `prepare`, so that replacing
/// `mism.toml` fails after the file was placed in the writable world directory.
fn place_with_a_read_only_server_directory(
    world: impl FnOnce(&DatapackEnvironment) -> PathBuf,
    prepare: impl FnOnce(&DatapackEnvironment),
) -> (DatapackEnvironment, Output, WorldState) {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let world = world(&environment);
    let environment = environment.with_level_name(world.to_str().unwrap());
    prepare(&environment);
    let source = fetch_local_example(&environment, &example_datapack());
    let before = environment.world_state();
    fs::set_permissions(environment.server(), fs::Permissions::from_mode(0o555)).unwrap();

    let output = environment.mism_datapack(["place", &source]).run();

    fs::set_permissions(environment.server(), fs::Permissions::from_mode(0o755)).unwrap();
    (environment, output, before)
}

/// d-dpdir-2
#[test]
fn failed_placement_removes_the_parent_it_created() {
    let (environment, output, _before) = place_with_a_read_only_server_directory(
        |environment| environment.outside().join("worlds/survival"),
        |_| {},
    );

    assert_eq!(
        (
            output.exit_code,
            environment.outside().join("worlds").exists()
        ),
        (1, false),
        "stdout: {}",
        output.stdout
    );
}

/// dp-11: when writing `mism.toml` fails after the file was placed, the existing world
/// directory and `mism.toml` are as they were.
#[test]
fn failure_to_write_mism_toml_leaves_the_world_as_it_was() {
    let (environment, output, before) = place_with_a_read_only_server_directory(
        |environment| environment.outside().join("existing-world"),
        |environment| {
            environment.put_in_datapacks("Other.zip", &example_datapack());
            fs::write(environment.world().join("level.dat"), b"level").unwrap();
        },
    );

    assert_eq!(
        (output.exit_code, environment.world_state()),
        (1, before),
        "stdout: {}",
        output.stdout
    );
}

/// dp-11: when writing `mism.toml` fails, nothing is at the path of the world directory, at
/// which nothing was before.
#[test]
fn failure_to_write_mism_toml_leaves_no_world_directory() {
    let (environment, output, _before) = place_with_a_read_only_server_directory(
        |environment| environment.outside().join("new-world"),
        |_| {},
    );

    assert_eq!(
        (
            output.exit_code,
            fs::symlink_metadata(environment.world()).is_err()
        ),
        (1, true),
        "stdout: {}",
        output.stdout
    );
}

/// dp-11: the failure to write `mism.toml` is an error.
#[test]
fn failure_to_write_mism_toml_is_an_error() {
    let (_environment, output, _before) = place_with_a_read_only_server_directory(
        |environment| environment.outside().join("new-world"),
        |_| {},
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// Makes `<world>/datapacks` a file.
fn make_datapacks_a_file(environment: &DatapackEnvironment) {
    fs::create_dir_all(environment.world()).unwrap();
    fs::write(environment.datapacks(), b"not a directory").unwrap();
}

/// d-dpdir-3: `<world>/datapacks` that is a file is an error.
#[test]
fn datapacks_that_is_a_file_is_an_error() {
    let (_environment, output, _before) = place_example(&example_datapack(), make_datapacks_a_file);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// d-dpdir-3: `<world>/datapacks` that is a file changes nothing.
#[test]
fn datapacks_that_is_a_file_changes_nothing() {
    let (environment, _output, before) = place_example(&example_datapack(), make_datapacks_a_file);

    assert_eq!(environment.world_state(), before);
}
