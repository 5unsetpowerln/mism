mod common;

use std::fs;
use std::path::{Path, PathBuf};

use nix::sys::signal::Signal;
use serde_json::{Value, json};

use common::datapack_server_directory::DatapackEnvironment;
use common::fake_modrinth::{
    ServedFile, TRUE_ENDING_FILE_NAME, TRUE_ENDING_NAME, TRUE_ENDING_SOURCE, saved_project,
    saved_version_list, sha256_hex,
};
use common::management_environment::{Record, mism_toml_with_records};
use common::mism_process::Output;
use common::plugin_output::{described, last_record, reported_an_error};
use common::stdin_connection::StdinConnection;
use common::test_datapack_zip::{
    datapack_declaring, datapack_with_pack_mcmeta, true_ending_pack_mcmeta, zip,
};

const TAKES_EFFECT_NOTICE: &str = "the change takes effect when the server starts next";
/// The file of the newer of True Ending's two datapack versions numbered 1.1.4b, for Minecraft
/// 1.21.2 to 1.21.10.
const TRUE_ENDING_1_1_4B: &str = "tru.e-ending-1.1.4b.zip";
/// The file of the older of True Ending's two datapack versions numbered 1.1.4b, for Minecraft
/// 1.21.2 to 1.21.8.
const TRUE_ENDING_1_1_4B_OLD: &str = "tru.e-ending-v1.1.4b-1.21.2-5.zip";
/// The file of True Ending's datapack version 1.1.4, whose number a mod version has too.
const TRUE_ENDING_1_1_4: &str = "true-ending-v1.1.4-1.21.2-5.zip";
const STELLARITY_PATCH: &str = "stellarity_x_true_ending_patch";
const MECHANICS_PATCH: &str = "Stellarity_Mechanics_Patch.zip";
/// The subcommands that need a target server (dp-1).
const SUBCOMMANDS_WITH_A_TARGET_SERVER: [&str; 4] = ["place", "install", "remove", "list"];
/// The question for the Minecraft version of `modrinth:true-ending:1.1.4b` (dp-5): the elements
/// of the lists of both datapack versions numbered 1.1.4b, in ascending string order.
const MINECRAFT_VERSION_QUESTION: &str = "mism: modrinth:true-ending:1.1.4b has files with these \
     values of minecraft_version:\nmism: 1.21.10\nmism: 1.21.2\nmism: 1.21.3\nmism: 1.21.4\n\
     mism: 1.21.5\nmism: 1.21.6\nmism: 1.21.7\nmism: 1.21.8\nmism: 1.21.9\n\
     mism: minecraft_version: ";

/// A datapack for exactly the datapack format 121, that of the test servers.
fn example_datapack() -> Vec<u8> {
    datapack_declaring(r#""min_format": 121, "max_format": 121"#)
}

fn true_ending_datapack() -> Vec<u8> {
    datapack_with_pack_mcmeta(&true_ending_pack_mcmeta())
}

/// A datapack of True Ending whose content differs for each `filename`, so that a fetch of one
/// file does not find another in the cache directory (dfetch-4).
fn true_ending_file(filename: &str) -> Vec<u8> {
    zip(&[
        ("pack.mcmeta", &true_ending_pack_mcmeta()),
        ("data/true_ending/file_name.txt", filename),
    ])
}

/// Serves each of `filenames` of True Ending's `versions` with its own content.
fn serve_true_ending_files(
    environment: &DatapackEnvironment,
    versions: &mut Value,
    filenames: &[&str],
) {
    let contents: Vec<Vec<u8>> = filenames
        .iter()
        .map(|filename| true_ending_file(filename))
        .collect();
    let served: Vec<ServedFile> = filenames
        .iter()
        .zip(&contents)
        .map(|(filename, bytes)| ServedFile::new(filename, bytes))
        .collect();
    environment.modrinth.serve_files(versions, &served);
}

/// The file name at the end of the cached file's path in a dfetch-5 record.
fn fetched_file_name(output: &Output) -> String {
    let record = last_record(&output.stdout);
    Path::new(record[4])
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

/// `environment` whose fake Modrinth serves True Ending's version list, with the datapack files
/// the tests fetch served, changed by `change`.
fn serving_true_ending_changed(
    environment: DatapackEnvironment,
    change: impl FnOnce(&mut Value),
) -> DatapackEnvironment {
    let mut versions = saved_version_list("true-ending");
    serve_true_ending_files(
        &environment,
        &mut versions,
        &[
            TRUE_ENDING_FILE_NAME,
            TRUE_ENDING_1_1_4B,
            TRUE_ENDING_1_1_4B_OLD,
            TRUE_ENDING_1_1_4,
        ],
    );
    change(&mut versions);
    environment.modrinth.mount_project("true-ending");
    environment
        .modrinth
        .mount_version_list_json("true-ending", &versions);
    environment
}

/// `environment` whose fake Modrinth serves True Ending, with the datapack files the tests fetch
/// served.
fn serving_true_ending(environment: DatapackEnvironment) -> DatapackEnvironment {
    serving_true_ending_changed(environment, |_| {})
}

/// The index in True Ending's saved version list of the Modrinth version with the file
/// `filename`.
fn index_of_file(versions: &Value, filename: &str) -> usize {
    versions
        .as_array()
        .unwrap()
        .iter()
        .position(|version| {
            version["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file["filename"] == filename)
        })
        .unwrap()
}

/// `mism datapack fetch <source>` from the served True Ending without a target server, with
/// stdin connected as `stdin` and the further arguments `extra`.
fn fetch_true_ending(source: &str, stdin: StdinConnection, extra: &[&str]) -> Output {
    let environment = serving_true_ending(DatapackEnvironment::without_server());
    environment
        .mism_datapack(["fetch", source])
        .arguments(extra)
        .stdin(stdin)
        .run()
}

/// `mism datapack fetch <url> --yes` of `example_datapack` served at `url_path`.
fn fetch_from(url_path: &str) -> (DatapackEnvironment, Output) {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(url_path, &example_datapack());
    let output = environment.fetch(&url);
    (environment, output)
}

/// An environment without `mism.toml` (but with `server.properties`) in whose cache directory
/// the local `Example.zip` was fetched. Returns the path of the local file too.
fn example_fetched_without_mism_toml() -> (DatapackEnvironment, PathBuf) {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("Example.zip", &example_datapack());
    environment.fetch(zip.to_str().unwrap());
    (environment, zip)
}

/// dp-1: `mism datapack place` without `mism.toml` is an error.
#[test]
fn place_without_mism_toml_is_an_error() {
    let (environment, zip) = example_fetched_without_mism_toml();

    let output = environment
        .mism_datapack(["place", zip.to_str().unwrap()])
        .run();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-1 (「mism plugin と mism datapack の対象のサーバーと共通の手順」 方法 2): `mism datapack
/// place` without `mism.toml` creates no world directory.
#[test]
fn place_without_mism_toml_creates_no_world_directory() {
    let (environment, zip) = example_fetched_without_mism_toml();
    let before = environment.world_state();

    environment
        .mism_datapack(["place", zip.to_str().unwrap()])
        .run();

    assert_eq!(environment.world_state(), before);
}

/// dp-1: `mism datapack install` without `mism.toml` is an error.
#[test]
fn install_without_mism_toml_is_an_error() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    let output = environment
        .mism_datapack(["install", TRUE_ENDING_SOURCE, "--yes"])
        .run();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-1 (「mism plugin と mism datapack の対象のサーバーと共通の手順」 方法 2): without
/// `mism.toml`, `mism datapack install` sends no request.
#[test]
fn install_without_mism_toml_sends_no_request() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    environment
        .mism_datapack(["install", TRUE_ENDING_SOURCE, "--yes"])
        .run();

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// dp-1: `mism datapack remove` without `mism.toml` is an error.
#[test]
fn remove_without_mism_toml_is_an_error() {
    let environment = DatapackEnvironment::without_server();

    let output = environment.mism_datapack(["remove", "Example"]).run();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-1: `mism datapack list` without `--global` and without `mism.toml` is an error.
#[test]
fn list_without_mism_toml_is_an_error() {
    let environment = DatapackEnvironment::without_server();

    let output = environment.mism_datapack(["list"]).run();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-2: `mism datapack inspect` without `mism.toml` goes on without a target server.
#[test]
fn inspect_without_mism_toml_succeeds() {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("Example.zip", &example_datapack());

    let output = environment
        .mism_datapack(["inspect", zip.to_str().unwrap()])
        .run();

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dp-2: without a target server there are no judgements.
#[test]
fn inspect_without_mism_toml_judges_nothing() {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("Example.zip", &example_datapack());

    let output = environment
        .mism_datapack(["inspect", zip.to_str().unwrap()])
        .run();

    assert_eq!(described(&output.stdout, "declared"), None);
}

/// dp-2: with `mism.toml` the current directory is the target server.
#[test]
fn inspect_with_mism_toml_judges_against_the_server() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let zip = environment.write_local_zip("Example.zip", &example_datapack());

    let output = environment
        .mism_datapack(["inspect", zip.to_str().unwrap()])
        .run();

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tunknown")
    );
}

/// `mism datapack place` of the fetched local `Example.zip` on a Paper 26.3 server whose
/// `level-name` is `level_name`.
fn place_example_with_level_name(level_name: &str) -> DatapackEnvironment {
    let environment = DatapackEnvironment::with_server("paper", "26.3").with_level_name(level_name);
    let zip = environment.write_local_zip("Example.zip", &example_datapack());
    environment.fetch(zip.to_str().unwrap());
    environment
        .mism_datapack(["place", zip.to_str().unwrap()])
        .run();
    environment
}

/// dp-3: a relative `level-name` is a directory below the server directory.
#[test]
fn relative_level_name_is_below_the_server_directory() {
    let environment = place_example_with_level_name("w2");

    assert_eq!(
        fs::read(environment.server().join("w2/datapacks/Example.zip")).ok(),
        Some(example_datapack())
    );
}

/// dp-3 (「プラグインとデータパックを置くディレクトリ」 方法): `..` in a relative `level-name` is
/// kept, so the world directory may be outside the server directory.
#[test]
fn level_name_with_dot_dot_is_taken_as_it_is() {
    let environment = place_example_with_level_name("../outside-world");

    assert_eq!(
        fs::read(
            environment
                .server()
                .join("../outside-world/datapacks/Example.zip")
        )
        .ok(),
        Some(example_datapack())
    );
}

/// dp-3: an absolute `level-name` is the world directory itself.
#[test]
fn absolute_level_name_is_the_world_directory() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let world = environment.outside().join("absolute-world");
    let environment = environment.with_level_name(world.to_str().unwrap());
    let zip = environment.write_local_zip("Example.zip", &example_datapack());
    environment.fetch(zip.to_str().unwrap());

    environment
        .mism_datapack(["place", zip.to_str().unwrap()])
        .run();

    assert_eq!(
        fs::read(world.join("datapacks/Example.zip")).ok(),
        Some(example_datapack())
    );
}

/// A Paper 26.3 server on which `mism datapack <subcommand>`, one of the subcommands of dp-1,
/// succeeds, and the arguments of that run.
fn ready_to_run(subcommand: &str) -> (DatapackEnvironment, Vec<String>) {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let zip = environment.write_local_zip("Example.zip", &example_datapack());
    let zip = zip.to_str().unwrap().to_owned();
    let arguments = match subcommand {
        "place" => {
            environment.fetch(&zip);
            vec!["place".to_owned(), zip]
        }
        "install" => vec!["install".to_owned(), zip, "--yes".to_owned()],
        "remove" => {
            environment.write_mism_toml(&mism_toml_with_records(
                "paper",
                "26.3",
                &[],
                &[Record {
                    name: "Example",
                    source: &zip,
                    file_name: "Example.zip",
                    sha256: &sha256_hex(&example_datapack()),
                }],
            ));
            vec!["remove".to_owned(), "Example".to_owned()]
        }
        "list" => vec!["list".to_owned()],
        _ => panic!("{subcommand} is not a subcommand of dp-1"),
    };
    (environment, arguments)
}

/// The subcommands of dp-1 that did not report an error when `prepare` had changed
/// `server.properties` of a server where they succeed otherwise.
fn subcommands_not_reporting_an_error(prepare: impl Fn(&DatapackEnvironment)) -> Vec<&'static str> {
    SUBCOMMANDS_WITH_A_TARGET_SERVER
        .into_iter()
        .filter(|subcommand| {
            let (environment, arguments) = ready_to_run(subcommand);
            prepare(&environment);
            !reported_an_error(&environment.mism_datapack(arguments).run())
        })
        .collect()
}

/// dp-3: with `level-name=world`, each subcommand of dp-1 succeeds in the runs that the next
/// tests break.
#[test]
fn subcommands_succeed_with_a_level_name() {
    let failed: Vec<&str> = SUBCOMMANDS_WITH_A_TARGET_SERVER
        .into_iter()
        .filter(|subcommand| {
            let (environment, arguments) = ready_to_run(subcommand);
            environment.mism_datapack(arguments).run().exit_code != 0
        })
        .collect();

    assert_eq!(failed, Vec::<&str>::new());
}

/// dp-3: without `server.properties`, each subcommand of dp-1 is an error.
#[test]
fn missing_server_properties_is_an_error() {
    let not_reporting =
        subcommands_not_reporting_an_error(DatapackEnvironment::remove_server_properties);

    assert_eq!(not_reporting, Vec::<&str>::new());
}

/// dp-3: a `server.properties` without `level-name` is an error for each subcommand of dp-1.
#[test]
fn server_properties_without_level_name_is_an_error() {
    let not_reporting = subcommands_not_reporting_an_error(|environment| {
        environment.write_server_properties(b"#Minecraft server properties\nmotd=A Server\n");
    });

    assert_eq!(not_reporting, Vec::<&str>::new());
}

/// dp-3: an empty `level-name` is an error for each subcommand of dp-1.
#[test]
fn empty_level_name_is_an_error() {
    let not_reporting = subcommands_not_reporting_an_error(|environment| {
        environment.write_server_properties(b"level-name=\nmotd=A Server\n");
    });

    assert_eq!(not_reporting, Vec::<&str>::new());
}

/// `mism datapack install modrinth:true-ending:1.1.4d --yes` on a Paper 26.1.2 server whose
/// `server.properties` is not UTF-8.
fn install_with_server_properties_not_utf8() -> (DatapackEnvironment, Output) {
    let environment = DatapackEnvironment::with_server("paper", "26.1.2");
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());
    // `é` in ISO-8859-1.
    environment.write_server_properties(b"level-name=w\xe9rld\n");
    let output = environment
        .mism_datapack(["install", TRUE_ENDING_SOURCE, "--yes"])
        .run();
    (environment, output)
}

/// d-props-6: a `server.properties` that is not UTF-8 is an error.
#[test]
fn server_properties_not_utf8_is_an_error() {
    let (_environment, output) = install_with_server_properties_not_utf8();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// d-props-6: with a `server.properties` that is not UTF-8, `mism datapack install` sends no
/// request to Modrinth.
#[test]
fn server_properties_not_utf8_sends_no_request() {
    let (environment, _output) = install_with_server_properties_not_utf8();

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// dp-4: a `modrinth:` source with values is a source.
#[test]
fn modrinth_source_with_values_is_fetched() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b:1.21.9",
        StdinConnection::NeverClosing,
        &[],
    );

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dp-4: a URL starting with `http://` is a source.
#[test]
fn url_is_fetched() {
    let (_environment, output) = fetch_from("/downloads/Example.zip");

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dp-4: the path of a local datapack file is a source.
#[test]
fn local_datapack_is_fetched() {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("Example.zip", &example_datapack());

    let output = environment.fetch(zip.to_str().unwrap());

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dp-4: a string of none of the three forms is an error.
#[test]
fn string_of_no_form_is_an_error() {
    let environment = DatapackEnvironment::without_server();

    let output = environment.fetch("true-ending");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-4: an identifier shorter than 3 characters is an error.
#[test]
fn short_identifier_is_an_error() {
    let environment = DatapackEnvironment::without_server();

    let output = environment.fetch("modrinth:te");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-4 (「取得元の指定」 方法): an identifier with characters Modrinth does not use sends no
/// request.
#[test]
fn identifier_with_a_path_sends_no_request() {
    let environment = DatapackEnvironment::without_server();

    environment.fetch("modrinth:../search");

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// dp-4: an identifier is the slug, so one that differs from it in case is an error.
#[test]
fn identifier_differing_from_the_slug_in_case_is_an_error() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .mount_project_json("True-Ending", &saved_project("true-ending"));

    let output = environment.fetch("modrinth:True-Ending:1.1.4d");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-4: an identifier of no Modrinth project is an error.
#[test]
fn identifier_of_no_project_is_an_error() {
    let environment = DatapackEnvironment::without_server();

    let output = environment.fetch("modrinth:no-such-datapack");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// d-source-4: a local file whose name does not end with `.zip` is an error.
#[test]
fn local_file_not_named_zip_is_an_error() {
    let environment = DatapackEnvironment::without_server();
    let file = environment.write_local_zip("pack.jar", &example_datapack());

    let output = environment.fetch(file.to_str().unwrap());

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// d-source-4: such a file is not stored.
#[test]
fn local_file_not_named_zip_is_not_stored() {
    let environment = DatapackEnvironment::without_server();
    let file = environment.write_local_zip("pack.jar", &example_datapack());

    environment.fetch(file.to_str().unwrap());

    assert_eq!(environment.files_in_cache(), Vec::<PathBuf>::new());
}

/// dp-5: the first value is the version number, and the candidates are the files of the
/// datapack versions, not those of the mod version with the same number.
#[test]
fn first_value_is_the_version_number_of_a_datapack_version() {
    let output = fetch_true_ending(TRUE_ENDING_SOURCE, StdinConnection::NeverClosing, &[]);

    assert_eq!(fetched_file_name(&output), TRUE_ENDING_FILE_NAME);
}

/// d-select-6
#[test]
fn version_number_of_a_mod_version_too_names_the_datapack_file() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4",
        StdinConnection::NeverClosing,
        &[],
    );

    assert_eq!(fetched_file_name(&output), TRUE_ENDING_1_1_4);
}

/// d-select-5
#[test]
fn file_with_a_file_type_is_no_candidate() {
    let environment = DatapackEnvironment::without_server();
    environment.modrinth.mount_project("navas-zamega");
    environment.modrinth.mount_version_list(
        "navas-zamega",
        &[ServedFile::new("ZA_ENfor1.7.zip", &example_datapack())],
    );

    let output = environment.fetch("modrinth:navas-zamega:1.2.1");

    assert_eq!(fetched_file_name(&output), "ZA_ENfor1.7.zip");
}

/// dp-5, dp-6: without values, the only version number of the candidates is the version number,
/// and the attributes with one kind of value are left out of the source.
#[test]
fn only_version_number_is_taken_without_values() {
    let environment =
        serving_true_ending_changed(DatapackEnvironment::without_server(), |versions| {
            versions
                .as_array_mut()
                .unwrap()
                .retain(|version| version["version_number"] == "1.1.4d");
        });

    let output = environment.fetch("modrinth:true-ending");

    assert_eq!(last_record(&output.stdout)[2], TRUE_ENDING_SOURCE);
}

/// dp-5: the Minecraft versions, which differ between the two versions numbered 1.1.4b, take the
/// next value, an element of their lists.
#[test]
fn next_value_narrows_the_minecraft_versions() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b:1.21.9",
        StdinConnection::NeverClosing,
        &[],
    );

    assert_eq!(fetched_file_name(&output), TRUE_ENDING_1_1_4B);
}

/// dp-5: a Minecraft version in both lists leaves both files, which the file name tells apart.
#[test]
fn minecraft_version_in_both_lists_leaves_the_file_name_to_choose() {
    let output = fetch_true_ending(
        &format!("modrinth:true-ending:1.1.4b:1.21.2:{TRUE_ENDING_1_1_4B_OLD}"),
        StdinConnection::NeverClosing,
        &[],
    );

    assert_eq!(fetched_file_name(&output), TRUE_ENDING_1_1_4B_OLD);
}

/// dp-5: the release type comes after the Minecraft versions.
#[test]
fn release_type_narrows_after_the_minecraft_versions() {
    let environment =
        serving_true_ending_changed(DatapackEnvironment::without_server(), |versions| {
            let index = index_of_file(versions, TRUE_ENDING_1_1_4B_OLD);
            versions[index]["version_type"] = "beta".into();
        });

    let output = environment.fetch("modrinth:true-ending:1.1.4b:1.21.2:beta");

    assert_eq!(fetched_file_name(&output), TRUE_ENDING_1_1_4B_OLD);
}

/// dp-5: the software is no attribute of datapacks, so different software lists ask for no
/// value.
#[test]
fn software_is_no_attribute_of_datapacks() {
    let environment =
        serving_true_ending_changed(DatapackEnvironment::without_server(), |versions| {
            let index = index_of_file(versions, TRUE_ENDING_1_1_4B);
            versions[index]["loaders"] = json!(["datapack", "minecraft"]);
        });

    let output = environment.fetch("modrinth:true-ending:1.1.4b:1.21.9");

    assert_eq!(fetched_file_name(&output), TRUE_ENDING_1_1_4B);
}

/// `environment` whose fake Modrinth serves the Stellarity patch, with its file
/// `Stellarity_Mechanics_Patch.zip` served, and the projects of its required dependencies.
fn serving_stellarity_patch(environment: DatapackEnvironment) -> DatapackEnvironment {
    environment.modrinth.mount_project(STELLARITY_PATCH);
    environment.modrinth.mount_version_list(
        STELLARITY_PATCH,
        &[ServedFile::new(MECHANICS_PATCH, &example_datapack())],
    );
    environment
        .modrinth
        .mount_projects("stellarity-true-ending", &["MCnBYP0b", "bZgeDzN8"]);
    environment
}

/// dp-5: the two versions numbered 1.0.0 of the Stellarity patch have the same list of Minecraft
/// versions, which counts as one kind, so the next value given is the file name.
#[test]
fn equal_lists_are_one_kind_of_value() {
    let environment = serving_stellarity_patch(DatapackEnvironment::without_server());

    let output = environment.fetch(&format!(
        "modrinth:{STELLARITY_PATCH}:1.0.0:{MECHANICS_PATCH}"
    ));

    assert_eq!(
        last_record(&output.stdout)[2],
        format!("modrinth:{STELLARITY_PATCH}:1.0.0:{MECHANICS_PATCH}")
    );
}

/// dp-5: a value that no candidate has is an error.
#[test]
fn value_that_no_candidate_has_is_an_error() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b:26.3",
        StdinConnection::NeverClosing,
        &[],
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-5: a value that no candidate has downloads nothing.
#[test]
fn value_that_no_candidate_has_downloads_nothing() {
    let environment = serving_true_ending(DatapackEnvironment::without_server());

    environment.fetch("modrinth:true-ending:1.1.4b:26.3");

    let downloads: Vec<String> = environment
        .modrinth
        .request_paths()
        .into_iter()
        .filter(|path| path.starts_with("/files/"))
        .collect();
    assert_eq!(downloads, Vec::<String>::new());
}

/// dp-5: a value left after the candidates are one file is an error.
#[test]
fn value_left_over_is_an_error() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4d:release",
        StdinConnection::NeverClosing,
        &[],
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-5: on a terminal, mism asks for the Minecraft version with the candidates' values.
#[test]
fn terminal_is_asked_for_the_minecraft_version() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b",
        StdinConnection::Terminal(b"1.21.9\n".to_vec()),
        &[],
    );

    assert!(
        output.stdout.starts_with(MINECRAFT_VERSION_QUESTION),
        "stdout: {}",
        output.stdout
    );
}

/// dp-5: the version numbers of the datapack versions are shown newest first.
#[test]
fn version_numbers_are_shown_newest_first() {
    let output = fetch_true_ending(
        "modrinth:true-ending",
        StdinConnection::Terminal(b"1.1.4d\n".to_vec()),
        &[],
    );

    assert!(
        output.stdout.starts_with(
            "mism: modrinth:true-ending has files with these values of version:\n\
             mism: 1.1.4d\nmism: 1.1.4c\nmism: 1.1.4b\nmism: 1.1.4a\nmism: 1.1.4\n"
        ),
        "stdout: {}",
        output.stdout
    );
}

/// dp-5, dp-6: the answers typed ahead choose the file whose source has them.
#[test]
fn answers_choose_the_file() {
    let output = fetch_true_ending(
        "modrinth:true-ending",
        StdinConnection::Terminal(b"1.1.4b\n1.21.9\n".to_vec()),
        &[],
    );

    assert_eq!(
        last_record(&output.stdout)[2],
        "modrinth:true-ending:1.1.4b:1.21.9"
    );
}

/// dp-5: an answer that is not one of the values is an error.
#[test]
fn answer_that_is_not_a_value_is_an_error() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b",
        StdinConnection::Terminal(b"26.3\n".to_vec()),
        &[],
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-5: an empty answer is an error.
#[test]
fn empty_answer_is_an_error() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b",
        StdinConnection::Terminal(b"\n".to_vec()),
        &[],
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-5: stdin that is not a terminal is not asked.
#[test]
fn stdin_that_is_not_a_terminal_is_not_asked() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b",
        StdinConnection::ClosedAfter(b"1.21.9\n".to_vec()),
        &[],
    );

    assert!(
        !output.stdout.contains("mism: minecraft_version: "),
        "stdout: {}",
        output.stdout
    );
}

/// dp-5: stdin that is not a terminal makes the narrowing an error.
#[test]
fn stdin_that_is_not_a_terminal_is_an_error() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b",
        StdinConnection::ClosedAfter(b"1.21.9\n".to_vec()),
        &[],
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-5: with `--yes`, a terminal is not asked.
#[test]
fn yes_keeps_a_terminal_from_being_asked() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b",
        StdinConnection::Terminal(b"1.21.9\n".to_vec()),
        &["--yes"],
    );

    assert!(
        !output.stdout.contains("mism: minecraft_version: "),
        "stdout: {}",
        output.stdout
    );
}

/// dp-5: with `--yes`, the narrowing that needs a value is an error.
#[test]
fn yes_makes_the_narrowing_an_error() {
    let output = fetch_true_ending(
        "modrinth:true-ending:1.1.4b",
        StdinConnection::Terminal(b"1.21.9\n".to_vec()),
        &["--yes"],
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-6: the source mism printed chooses the same file without asking.
#[test]
fn printed_source_chooses_the_same_file_without_asking() {
    let environment = serving_true_ending(DatapackEnvironment::without_server());
    let asked = environment
        .mism_datapack(["fetch", "modrinth:true-ending"])
        .stdin(StdinConnection::Terminal(
            format!("1.1.4b\n1.21.2\n{TRUE_ENDING_1_1_4B_OLD}\n").into_bytes(),
        ))
        .run();
    let printed_source = last_record(&asked.stdout)[2].to_owned();

    let output = environment
        .mism_datapack(["fetch", &printed_source])
        .stdin(StdinConnection::NeverClosing)
        .run();

    assert_eq!(fetched_file_name(&output), fetched_file_name(&asked));
}

/// dp-8: a datapack from Modrinth is named by its name on Modrinth.
#[test]
fn modrinth_datapack_name_is_its_title() {
    let output = fetch_true_ending(TRUE_ENDING_SOURCE, StdinConnection::NeverClosing, &[]);

    assert_eq!(last_record(&output.stdout)[0], TRUE_ENDING_NAME);
}

/// dp-8: a datapack from a URL is named as dfetch-2 decides.
#[test]
fn url_datapack_name_is_made_from_the_url() {
    let (_environment, output) = fetch_from("/downloads/Example.zip");

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// dp-8: a local datapack file is named by its file name without `.zip`.
#[test]
fn local_datapack_name_is_the_file_name_without_zip() {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("My.Pack.zip", &example_datapack());

    let output = environment.fetch(zip.to_str().unwrap());

    assert_eq!(last_record(&output.stdout)[0], "My.Pack");
}

/// dp-9: an empty datapack name is an error.
#[test]
fn empty_datapack_name_is_an_error() {
    let (_environment, output) = fetch_from("/downloads/.zip");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-9: the datapack name `.` is an error.
#[test]
fn dot_datapack_name_is_an_error() {
    let (_environment, output) = fetch_from("/downloads/..zip");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-9: the datapack name `..` is an error.
#[test]
fn dot_dot_datapack_name_is_an_error() {
    let (_environment, output) = fetch_from("/downloads/...zip");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-9: the datapack name `..` stores nothing.
#[test]
fn dot_dot_datapack_name_stores_nothing() {
    let (environment, _output) = fetch_from("/downloads/...zip");

    assert_eq!(environment.files_in_cache(), Vec::<PathBuf>::new());
}

/// dp-9: a datapack name from Modrinth is checked too.
#[test]
fn dot_dot_title_is_an_error() {
    let environment = DatapackEnvironment::without_server();
    let mut project = saved_project("true-ending");
    project["title"] = "..".into();
    environment
        .modrinth
        .mount_project_json("true-ending", &project);
    environment.modrinth.mount_version_list(
        "true-ending",
        &[ServedFile::new(
            TRUE_ENDING_FILE_NAME,
            &true_ending_datapack(),
        )],
    );

    let output = environment.fetch(TRUE_ENDING_SOURCE);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dp-10: `mism datapack place` ends with the notice.
#[test]
fn place_ends_with_the_notice() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let zip = environment.write_local_zip("Example.zip", &example_datapack());
    environment.fetch(zip.to_str().unwrap());

    let output = environment
        .mism_datapack(["place", zip.to_str().unwrap()])
        .run();

    assert_eq!(
        output.stdout.lines().last(),
        Some(format!("mism: {TAKES_EFFECT_NOTICE}").as_str())
    );
}

/// dp-12: the values of a record are separated by tabs, and an absent value is `-` (the version
/// number of a URL source).
#[test]
fn record_values_are_separated_by_tabs_with_a_hyphen_for_none() {
    let (_environment, output) = fetch_from("/downloads/Example.zip");

    assert_eq!(last_record(&output.stdout)[1], "-");
}

/// dp-12: the elements of a list value are separated by `,`.
#[test]
fn list_elements_are_separated_by_commas() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    let output = environment
        .mism_datapack(["inspect", TRUE_ENDING_SOURCE])
        .run();

    assert_eq!(
        described(&output.stdout, "modrinth.minecraft_versions"),
        Some(
            "modrinth.minecraft_versions\t1.21.2,1.21.3,1.21.4,1.21.5,1.21.6,1.21.7,1.21.8,1.21.9,\
             1.21.10,1.21.11,26.1,26.1.1,26.1.2"
        )
    );
}

/// dp-12: a list without elements is `-`.
#[test]
fn empty_list_is_a_hyphen() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    let output = environment
        .mism_datapack(["inspect", TRUE_ENDING_SOURCE])
        .run();

    assert_eq!(
        described(&output.stdout, "modrinth.required_dependencies"),
        Some("modrinth.required_dependencies\t-")
    );
}

/// d-pcmd-2: a subcommand interrupted while it asks leaves the world directory and `mism.toml` as
/// they were.
#[test]
fn interrupted_subcommand_leaves_the_world_as_it_was() {
    let environment = serving_true_ending(DatapackEnvironment::with_server("paper", "26.3"));
    environment.put_in_datapacks("Other.zip", &example_datapack());
    let before = environment.world_state();
    let mut mism = environment
        .mism_datapack(["install", "modrinth:true-ending:1.1.4b"])
        .stdin(StdinConnection::Terminal(Vec::new()))
        .spawn();
    mism.wait_for_stdout("mism: minecraft_version: ");

    mism.send(Signal::SIGINT);
    mism.wait();

    assert_eq!(environment.world_state(), before);
}
