mod common;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use nix::sys::signal::Signal;
use serde_json::Value;

use common::datapack_server_directory::{DatapackEnvironment, WorldState};
use common::fake_modrinth::{
    ServedFile, TRUE_ENDING_FILE_NAME, TRUE_ENDING_NAME, TRUE_ENDING_SOURCE, saved_version_list,
    sha256_hex,
};
use common::management_environment::{Record, mism_toml_with_records};
use common::mism_process::Output;
use common::plugin_output::{described, has_line, last_record, line_index, reported_an_error};
use common::stdin_connection::StdinConnection;
use common::test_datapack_zip::{
    datapack_declaring, not_a_zip_file, true_ending_pack_mcmeta, zip,
    zip_with_pack_mcmeta_in_a_directory,
};

const TAKES_EFFECT_NOTICE: &str = "the change takes effect when the server starts next";
const EXAMPLE_PATH: &str = "/downloads/Example.zip";
const NOT_PLACING: &str = "not placing Example.zip";
const TRUE_ENDING_1_1_4B: &str = "tru.e-ending-1.1.4b.zip";
const TRUE_ENDING_1_1_4C: &str = "tru.e-ending-1.1.4c.zip";
/// The file of the newest datapack version of True Ending for Minecraft 1.21.1.
const TRUE_ENDING_1_1_2D: &str = "true-ending-v1.1.2d.zip";
/// The file of the datapack version 1.1.1 of True Ending for Minecraft 1.20.5 and 1.20.6.
const TRUE_ENDING_1_1_1_FOR_1_20: &str = "trueending_v1.1.1_1.20.6.zip";
/// The file of the other datapack version numbered 1.1.1, for Minecraft 1.21.
const TRUE_ENDING_1_1_1_FOR_1_21: &str = "true-ending-v1.1.1.zip";
/// A second file that a test adds to the datapack version 1.1.4d.
const TRUE_ENDING_1_1_4D_EXTRA: &str = "tru.e-ending-1.1.4d-extra.zip";
/// The source of the file `--lazy` chooses on a server for Minecraft 1.20.6: the Minecraft
/// version tells apart the two datapack versions numbered 1.1.1 (「Modrinth のファイルの選択」
/// 方法 9).
const TRUE_ENDING_LAZY_SOURCE_FOR_1_20_6: &str = "modrinth:true-ending:1.1.1:1.20.6";

/// A datapack for exactly the datapack format 121, that of the test servers.
fn example_datapack() -> Vec<u8> {
    datapack_declaring(r#""min_format": 121, "max_format": 121"#)
}

/// A datapack of True Ending whose content differs for each `filename`.
fn true_ending_file(filename: &str) -> Vec<u8> {
    zip(&[
        ("pack.mcmeta", &true_ending_pack_mcmeta()),
        ("data/true_ending/file_name.txt", filename),
    ])
}

/// A server of `minecraft_version` whose fake Modrinth serves True Ending's version list, changed
/// by `change` after each of `filenames` was served with its own content (`true_ending_file`).
fn serving_true_ending_changed(
    minecraft_version: &str,
    filenames: &[&str],
    change: impl FnOnce(&mut Value),
) -> DatapackEnvironment {
    let environment = DatapackEnvironment::with_server("paper", minecraft_version);
    let contents: Vec<Vec<u8>> = filenames
        .iter()
        .map(|filename| true_ending_file(filename))
        .collect();
    let served: Vec<ServedFile> = filenames
        .iter()
        .zip(&contents)
        .map(|(filename, bytes)| ServedFile::new(filename, bytes))
        .collect();
    let mut versions = saved_version_list("true-ending");
    environment.modrinth.serve_files(&mut versions, &served);
    change(&mut versions);
    environment.modrinth.mount_project("true-ending");
    environment
        .modrinth
        .mount_version_list_json("true-ending", &versions);
    environment
}

/// A server of `minecraft_version` whose fake Modrinth serves True Ending, with the files the
/// tests install served.
fn serving_true_ending(minecraft_version: &str) -> DatapackEnvironment {
    serving_true_ending_changed(
        minecraft_version,
        &[
            TRUE_ENDING_FILE_NAME,
            TRUE_ENDING_1_1_4B,
            TRUE_ENDING_1_1_4C,
            TRUE_ENDING_1_1_2D,
            TRUE_ENDING_1_1_1_FOR_1_20,
            TRUE_ENDING_1_1_1_FOR_1_21,
        ],
        |_| {},
    )
}

/// `mism datapack install <source> <extra>` in `environment` with stdin connected as `stdin`.
fn install(
    environment: &DatapackEnvironment,
    source: &str,
    extra: &[&str],
    stdin: StdinConnection,
) -> Output {
    environment
        .mism_datapack(["install", source])
        .arguments(extra)
        .stdin(stdin)
        .run()
}

/// `mism datapack install <source> <extra>` of True Ending on a server for `minecraft_version`.
fn install_true_ending(
    minecraft_version: &str,
    source: &str,
    extra: &[&str],
    stdin: StdinConnection,
) -> (DatapackEnvironment, Output) {
    let environment = serving_true_ending(minecraft_version);
    let output = install(&environment, source, extra, stdin);
    (environment, output)
}

/// `mism datapack install modrinth:true-ending --lazy --yes` on a server for
/// `minecraft_version`.
fn install_true_ending_lazily(minecraft_version: &str) -> (DatapackEnvironment, Output) {
    install_true_ending(
        minecraft_version,
        "modrinth:true-ending",
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    )
}

/// Installs the local `Example.zip` with `bytes` on `environment` prepared by `prepare`, with the
/// further arguments `extra` and stdin connected as `stdin`. Returns the state of the world
/// directory and `mism.toml` before the installation too.
fn install_local_example_in(
    environment: DatapackEnvironment,
    bytes: &[u8],
    prepare: impl FnOnce(&DatapackEnvironment),
    extra: &[&str],
    stdin: StdinConnection,
) -> (DatapackEnvironment, Output, WorldState) {
    prepare(&environment);
    let zip = environment.write_local_zip("Example.zip", bytes);
    let before = environment.world_state();
    let output = install(&environment, zip.to_str().unwrap(), extra, stdin);
    (environment, output, before)
}

/// `install_local_example_in` on a Paper 26.3 server whose datapack format is 121.0.
fn install_local_example(
    bytes: &[u8],
    prepare: impl FnOnce(&DatapackEnvironment),
    extra: &[&str],
    stdin: StdinConnection,
) -> (DatapackEnvironment, Output, WorldState) {
    install_local_example_in(
        DatapackEnvironment::with_server("paper", "26.3"),
        bytes,
        prepare,
        extra,
        stdin,
    )
}

/// Installs the local `Example.zip` with `bytes` on a server prepared by `prepare`, with `--yes`.
fn install_local_example_with_yes(
    bytes: &[u8],
    prepare: impl FnOnce(&DatapackEnvironment),
) -> (DatapackEnvironment, Output, WorldState) {
    install_local_example(bytes, prepare, &["--yes"], StdinConnection::NeverClosing)
}

/// Installs the local `Example.zip`, answering the confirmation with `typed` on a terminal.
fn install_local_example_answering(typed: &[u8]) -> (DatapackEnvironment, Output, WorldState) {
    install_local_example(
        &example_datapack(),
        |_| {},
        &[],
        StdinConnection::Terminal(typed.to_vec()),
    )
}

/// Installs `Example.zip` from a URL on a Paper server for 26.3, with the further arguments
/// `extra` and stdin connected as `stdin`.
fn install_example_from_a_url(
    extra: &[&str],
    stdin: StdinConnection,
) -> (DatapackEnvironment, Output) {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());
    let output = install(&environment, &url, extra, stdin);
    (environment, output)
}

/// The question whether to place `file_name` in the world directory of `environment` (dinst-5).
fn question(environment: &DatapackEnvironment, file_name: &str) -> String {
    format!(
        "place {file_name} in {}/datapacks/? (y/n): ",
        environment.world().display()
    )
}

/// The parsed text of a `mism.toml`, to compare what it says rather than how it is formatted.
fn parsed(text: &str) -> toml::Table {
    text.parse().unwrap()
}

fn placed(environment: &DatapackEnvironment, file_name: &str) -> Option<Vec<u8>> {
    fs::read(environment.datapacks().join(file_name)).ok()
}

/// `mism.toml` with a datapack record of `Example` whose file is `Old.zip`.
fn record_named_example(environment: &DatapackEnvironment) {
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

/// A file named `Example.zip` in `datapacks/`, of other content.
fn put_file_named_example(environment: &DatapackEnvironment) {
    environment.put_in_datapacks("Example.zip", b"other");
}

/// A directory named `Example.zip` in `datapacks/`.
fn put_directory_named_example(environment: &DatapackEnvironment) {
    fs::create_dir_all(environment.datapacks().join("Example.zip")).unwrap();
}

/// A symbolic link named `Example.zip` without a target in `datapacks/`.
fn put_dangling_symlink_named_example(environment: &DatapackEnvironment) {
    fs::create_dir_all(environment.datapacks()).unwrap();
    symlink("no-such-file", environment.datapacks().join("Example.zip")).unwrap();
}

/// `mism datapack install modrinth:true-ending --lazy --yes` on a server for 26.3 whose fake
/// Modrinth serves only True Ending's datapack version 1.1.4d, which is for other Minecraft
/// versions. Without `--lazy`, mism would install that only version.
fn install_the_only_version_lazily_on_26_3() -> (DatapackEnvironment, Output) {
    let environment = serving_true_ending_changed("26.3", &[TRUE_ENDING_FILE_NAME], |versions| {
        let only = versions[1].clone();
        *versions = Value::Array(vec![only]);
    });
    let output = install(
        &environment,
        "modrinth:true-ending",
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    );
    (environment, output)
}

/// Installs the local `Example.zip` with nothing typed on its terminal, sends `signal` once the
/// confirmation is asked, and returns the output and the state before the run.
fn signal_at_the_confirmation(signal: Signal) -> (DatapackEnvironment, Output, WorldState) {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    environment.put_in_datapacks("Other.zip", b"other");
    let zip = environment.write_local_zip("Example.zip", &example_datapack());
    let before = environment.world_state();
    let mut mism = environment
        .mism_datapack(["install", zip.to_str().unwrap()])
        .stdin(StdinConnection::Terminal(Vec::new()))
        .spawn();
    mism.wait_for_stdout(&format!("mism: {}", question(&environment, "Example.zip")));

    mism.send(signal);
    let output = mism.wait();
    (environment, output, before)
}

/// dinst-1: the file a `modrinth:` source names is installed.
#[test]
fn modrinth_file_is_installed() {
    let (environment, _output) = install_true_ending(
        "26.1.2",
        TRUE_ENDING_SOURCE,
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(
        placed(&environment, TRUE_ENDING_FILE_NAME),
        Some(true_ending_file(TRUE_ENDING_FILE_NAME))
    );
}

/// dinst-1: without `--lazy`, the files are narrowed as dp-5 says, by the answers on a terminal.
#[test]
fn modrinth_file_is_chosen_by_the_answers() {
    let (environment, _output) = install_true_ending(
        "26.1.2",
        "modrinth:true-ending:1.1.4b",
        &[],
        StdinConnection::Terminal(b"1.21.9\ny\n".to_vec()),
    );

    assert_eq!(
        placed(&environment, TRUE_ENDING_1_1_4B),
        Some(true_ending_file(TRUE_ENDING_1_1_4B))
    );
}

/// dinst-1: the datapack file of a URL is installed.
#[test]
fn url_file_is_installed() {
    let (environment, _output) =
        install_example_from_a_url(&["--yes"], StdinConnection::NeverClosing);

    assert_eq!(
        placed(&environment, "Example.zip"),
        Some(example_datapack())
    );
}

/// dinst-1: a local datapack file is installed.
#[test]
fn local_file_is_installed() {
    let (environment, _output, _before) =
        install_local_example_with_yes(&example_datapack(), |_| {});

    assert_eq!(
        placed(&environment, "Example.zip"),
        Some(example_datapack())
    );
}

/// dinst-2: `--lazy` installs the newest datapack version whose Minecraft versions have the
/// server's.
#[test]
fn lazy_installs_the_newest_version_that_matches_the_server() {
    let (environment, _output) = install_true_ending_lazily("1.21.1");

    assert_eq!(
        placed(&environment, TRUE_ENDING_1_1_2D),
        Some(true_ending_file(TRUE_ENDING_1_1_2D))
    );
}

/// dinst-2: `--lazy` installs the newest matching version also when the list has an older one
/// first.
#[test]
fn lazy_installs_the_newest_matching_version_wherever_it_is_listed() {
    let environment = serving_true_ending_changed("1.21.1", &[TRUE_ENDING_1_1_2D], |versions| {
        versions.as_array_mut().unwrap().reverse();
    });

    install(
        &environment,
        "modrinth:true-ending",
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(
        placed(&environment, TRUE_ENDING_1_1_2D),
        Some(true_ending_file(TRUE_ENDING_1_1_2D))
    );
}

/// dinst-2: of the two datapack versions numbered 1.1.1, `--lazy` installs the one that matches
/// the server.
#[test]
fn lazy_installs_the_file_of_the_matching_version() {
    let (environment, _output) = install_true_ending_lazily("1.20.6");

    assert_eq!(
        placed(&environment, TRUE_ENDING_1_1_1_FOR_1_20),
        Some(true_ending_file(TRUE_ENDING_1_1_1_FOR_1_20))
    );
}

/// dinst-2: after `--lazy` chose the version, the files are narrowed as dp-5 says: here the
/// chosen version has two files.
#[test]
fn lazy_asks_for_the_attributes_after_the_version() {
    let environment = serving_true_ending_changed("26.1.2", &[TRUE_ENDING_FILE_NAME], |versions| {
        // The datapack version 1.1.4d gets a copy of its served file under another name.
        let mut extra = versions[1]["files"][0].clone();
        extra["filename"] = TRUE_ENDING_1_1_4D_EXTRA.into();
        extra["primary"] = false.into();
        versions[1]["files"].as_array_mut().unwrap().push(extra);
    });

    install(
        &environment,
        "modrinth:true-ending",
        &["--lazy"],
        StdinConnection::Terminal(format!("{TRUE_ENDING_FILE_NAME}\ny\n").into_bytes()),
    );

    assert_eq!(
        placed(&environment, TRUE_ENDING_FILE_NAME),
        Some(true_ending_file(TRUE_ENDING_FILE_NAME))
    );
}

/// dinst-2: without a matching version, `--lazy` is an error, also when the only version could
/// be chosen without `--lazy`.
#[test]
fn lazy_without_a_matching_version_is_an_error() {
    let (_environment, output) = install_the_only_version_lazily_on_26_3();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-2: without a matching version, mism chooses no other version, so it downloads nothing.
#[test]
fn lazy_without_a_matching_version_downloads_nothing() {
    let (environment, _output) = install_the_only_version_lazily_on_26_3();

    let downloads: Vec<String> = environment
        .modrinth
        .request_paths()
        .into_iter()
        .filter(|path| path.starts_with("/files/"))
        .collect();
    assert_eq!(downloads, Vec::<String>::new());
}

/// dinst-2, dp-6: the source of the file `--lazy` chose has the values that tell it apart.
#[test]
fn lazy_choice_has_the_source_of_its_file() {
    let (_environment, output) = install_true_ending_lazily("1.20.6");

    assert_eq!(
        described(&output.stdout, "source"),
        Some(format!("source\t{TRUE_ENDING_LAZY_SOURCE_FOR_1_20_6}").as_str())
    );
}

/// dinst-2, dp-6: the source printed for the file `--lazy` chose finds the same file without
/// `--lazy` and without asking.
#[test]
fn source_of_a_lazy_choice_names_the_same_file_without_lazy() {
    let (_environment, lazy) = install_true_ending_lazily("1.20.6");
    let printed_source = described(&lazy.stdout, "source").unwrap()["source\t".len()..].to_owned();
    let environment = serving_true_ending("1.20.6");

    let output = environment
        .mism_datapack(["fetch", &printed_source])
        .stdin(StdinConnection::NeverClosing)
        .run();

    let cached = last_record(&output.stdout)[4].to_owned();
    assert_eq!(
        Path::new(&cached).file_name().unwrap().to_str(),
        Some(TRUE_ENDING_1_1_1_FOR_1_20)
    );
}

/// dinst-2, dp-7: the record in `mism.toml` has the source of the file `--lazy` chose.
#[test]
fn lazy_choice_is_recorded_with_the_source_of_its_file() {
    let (environment, _output) = install_true_ending_lazily("1.20.6");

    assert_eq!(
        parsed(&environment.read_mism_toml()),
        parsed(&mism_toml_with_records(
            "paper",
            "1.20.6",
            &[],
            &[Record {
                name: TRUE_ENDING_NAME,
                source: TRUE_ENDING_LAZY_SOURCE_FOR_1_20_6,
                file_name: TRUE_ENDING_1_1_1_FOR_1_20,
                sha256: &sha256_hex(&true_ending_file(TRUE_ENDING_1_1_1_FOR_1_20)),
            }]
        ))
    );
}

/// dinst-3: `--lazy` with a URL is ignored.
#[test]
fn lazy_is_ignored_with_a_url() {
    let (environment, _output) =
        install_example_from_a_url(&["--lazy", "--yes"], StdinConnection::NeverClosing);

    assert_eq!(
        placed(&environment, "Example.zip"),
        Some(example_datapack())
    );
}

/// dinst-3: `--lazy` with a local datapack file is ignored.
#[test]
fn lazy_is_ignored_with_a_local_file() {
    let (environment, _output, _before) = install_local_example(
        &example_datapack(),
        |_| {},
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(
        placed(&environment, "Example.zip"),
        Some(example_datapack())
    );
}

/// dinst-3: `--lazy` with a version number is ignored: the version given is installed, not the
/// newest matching one, although it does not match.
#[test]
fn lazy_is_ignored_with_a_version_number() {
    let (environment, _output) = install_true_ending(
        "26.1.2",
        "modrinth:true-ending:1.1.4c",
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(
        placed(&environment, TRUE_ENDING_1_1_4C),
        Some(true_ending_file(TRUE_ENDING_1_1_4C))
    );
}

/// dinst-4: the output is the fetch, the inspection, the question, the placement, and the
/// notice, in this order.
#[test]
fn output_is_fetch_inspection_question_placement_and_notice() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());

    let output = install(
        &environment,
        &url,
        &[],
        StdinConnection::Terminal(b"\ny\n".to_vec()),
    );

    let question = question(&environment, "Example.zip");
    let indices: Vec<Option<usize>> = [
        "datapack name from the URL: Example",
        "name\tExample",
        "declared\tunknown",
        "pack_format\tmatch",
        question.as_str(),
        "Example\t-\tExample.zip",
        TAKES_EFFECT_NOTICE,
    ]
    .into_iter()
    .map(|text| line_index(&output.stdout, text))
    .collect();
    assert!(
        indices.iter().all(Option::is_some) && indices.is_sorted(),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-4, dp-7: after the installation, `mism.toml` has the record of the datapack.
#[test]
fn installed_datapack_is_recorded() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let zip = environment.write_local_zip("Example.zip", &example_datapack());
    let source = zip.to_str().unwrap();

    install(
        &environment,
        source,
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(
        parsed(&environment.read_mism_toml()),
        parsed(&mism_toml_with_records(
            "paper",
            "26.3",
            &[],
            &[Record {
                name: "Example",
                source,
                file_name: "Example.zip",
                sha256: &sha256_hex(&example_datapack()),
            }]
        ))
    );
}

/// dinst-4: a successful installation exits with status 0.
#[test]
fn successful_installation_exits_with_status_0() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_datapack(), |_| {});

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dinst-4: a `pack_format` of `mismatch` does not stop the installation.
#[test]
fn pack_format_mismatch_does_not_stop_the_installation() {
    let bytes = datapack_declaring(r#""pack_format": 48"#);

    let (environment, _output, _before) = install_local_example_with_yes(&bytes, |_| {});

    assert_eq!(placed(&environment, "Example.zip"), Some(bytes));
}

/// dinst-4, dinsp-4: a file that is not a zip file is an error.
#[test]
fn file_that_is_not_zip_is_an_error() {
    let (_environment, output, _before) = install_local_example_with_yes(&not_a_zip_file(), |_| {});

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dinsp-4: a file that is not a zip file changes nothing.
#[test]
fn file_that_is_not_zip_changes_nothing() {
    let (environment, _output, before) = install_local_example_with_yes(&not_a_zip_file(), |_| {});

    assert_eq!(environment.world_state(), before);
}

/// dinst-4 (「mism plugin install と mism datapack install の手順」 方法 5): for a file that is
/// not a zip file, mism writes no inspection.
#[test]
fn file_that_is_not_zip_is_not_inspected() {
    let (_environment, output, _before) = install_local_example_with_yes(&not_a_zip_file(), |_| {});

    assert_eq!(described(&output.stdout, "name"), None);
}

/// dinst-4, dinsp-4: a zip file without a top-level `pack.mcmeta` is an error.
#[test]
fn zip_without_a_top_level_pack_mcmeta_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&zip_with_pack_mcmeta_in_a_directory(), |_| {});

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dinsp-4: a zip file without a top-level `pack.mcmeta` changes nothing.
#[test]
fn zip_without_a_top_level_pack_mcmeta_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&zip_with_pack_mcmeta_in_a_directory(), |_| {});

    assert_eq!(environment.world_state(), before);
}

/// dinst-4, dinsp-4: a declaration with `min_format` and without `max_format` is an error.
#[test]
fn min_format_alone_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&datapack_declaring(r#""min_format": 121"#), |_| {});

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dinsp-4: a declaration with `min_format` and without `max_format` changes nothing.
#[test]
fn min_format_alone_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&datapack_declaring(r#""min_format": 121"#), |_| {});

    assert_eq!(environment.world_state(), before);
}

/// dinst-4, dinsp-5: a server without its server jar is an error.
#[test]
fn server_without_its_jar_is_an_error() {
    let (_environment, output, _before) = install_local_example_in(
        DatapackEnvironment::with_server_without_jar("paper", "26.3"),
        &example_datapack(),
        |_| {},
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dinsp-5: a server without its server jar changes nothing.
#[test]
fn server_without_its_jar_changes_nothing() {
    let (environment, _output, before) = install_local_example_in(
        DatapackEnvironment::with_server_without_jar("paper", "26.3"),
        &example_datapack(),
        |_| {},
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(environment.world_state(), before);
}

/// dinst-4, dplace-3: a record with the same name is an error.
#[test]
fn record_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_datapack(), record_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dplace-3: a record with the same name changes nothing.
#[test]
fn record_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&example_datapack(), record_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dinst-4, dplace-3 (「mism plugin install と mism datapack install の手順」 方法 6): a record
/// with the same name is found before the question.
#[test]
fn record_with_the_same_name_is_found_before_asking() {
    let (environment, output, _before) = install_local_example(
        &example_datapack(),
        record_named_example,
        &[],
        StdinConnection::Terminal(b"y\n".to_vec()),
    );

    assert!(
        !output
            .stdout
            .contains(&question(&environment, "Example.zip")),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-4, dplace-4: a file with the same name in `datapacks/` is an error.
#[test]
fn file_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_datapack(), put_file_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dplace-4: a file with the same name in `datapacks/` changes nothing.
#[test]
fn file_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&example_datapack(), put_file_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dinst-4, dplace-4: a directory with the same name in `datapacks/` is an error.
#[test]
fn directory_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_datapack(), put_directory_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dplace-4: a directory with the same name in `datapacks/` changes nothing.
#[test]
fn directory_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&example_datapack(), put_directory_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dinst-4, dplace-4: a symbolic link with the same name in `datapacks/`, without a target, is an
/// error.
#[test]
fn dangling_symlink_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_datapack(), put_dangling_symlink_named_example);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-4, dplace-4: a symbolic link with the same name in `datapacks/`, without a target,
/// changes nothing.
#[test]
fn dangling_symlink_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&example_datapack(), put_dangling_symlink_named_example);

    assert_eq!(environment.world_state(), before);
}

/// dinst-5: without `--yes`, mism asks whether to place the file in the world directory's
/// `datapacks/`, named by its absolute path.
#[test]
fn question_is_asked() {
    let (environment, output, _before) = install_local_example_answering(b"n\n");

    assert!(
        has_line(&output.stdout, &question(&environment, "Example.zip")),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-5: the question names the world directory of the `level-name`.
#[test]
fn question_names_the_world_directory_of_the_level_name() {
    let (environment, output, _before) = install_local_example_in(
        DatapackEnvironment::with_server("paper", "26.3").with_level_name("worlds/survival"),
        &example_datapack(),
        |_| {},
        &[],
        StdinConnection::Terminal(b"n\n".to_vec()),
    );

    assert!(
        has_line(
            &output.stdout,
            &format!(
                "place Example.zip in {}/worlds/survival/datapacks/? (y/n): ",
                environment.server().display()
            )
        ),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-5: the answer `y` places the file.
#[test]
fn answer_y_places_the_file() {
    let (environment, _output, _before) = install_local_example_answering(b"y\n");

    assert_eq!(
        placed(&environment, "Example.zip"),
        Some(example_datapack())
    );
}

/// dinst-5: the answer `yes` places the file.
#[test]
fn answer_yes_places_the_file() {
    let (environment, _output, _before) = install_local_example_answering(b"yes\n");

    assert_eq!(
        placed(&environment, "Example.zip"),
        Some(example_datapack())
    );
}

/// dinst-5: another answer writes that the file is not placed.
#[test]
fn other_answer_writes_that_the_file_is_not_placed() {
    let (_environment, output, _before) = install_local_example_answering(b"n\n");

    assert!(
        has_line(&output.stdout, NOT_PLACING),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-5: another answer changes nothing.
#[test]
fn other_answer_changes_nothing() {
    let (environment, _output, before) = install_local_example_answering(b"n\n");

    assert_eq!(environment.world_state(), before);
}

/// dinst-5: another answer exits with status 0.
#[test]
fn other_answer_exits_with_status_0() {
    let (_environment, output, _before) = install_local_example_answering(b"n\n");

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dinst-5: an empty answer is another answer.
#[test]
fn empty_answer_does_not_place_the_file() {
    let (_environment, output, _before) = install_local_example_answering(b"\n");

    assert!(
        has_line(&output.stdout, NOT_PLACING),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-5: `Y` is not `y`.
#[test]
fn capital_y_does_not_place_the_file() {
    let (_environment, output, _before) = install_local_example_answering(b"Y\n");

    assert!(
        has_line(&output.stdout, NOT_PLACING),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-6: with `--yes`, mism places the file without asking, whatever is typed.
#[test]
fn yes_places_the_file_without_asking() {
    let (environment, _output, _before) = install_local_example(
        &example_datapack(),
        |_| {},
        &["--yes"],
        StdinConnection::Terminal(b"n\n".to_vec()),
    );

    assert_eq!(
        placed(&environment, "Example.zip"),
        Some(example_datapack())
    );
}

/// dinst-6: with `--yes`, there is no question.
#[test]
fn yes_writes_no_question() {
    let (environment, output, _before) = install_local_example(
        &example_datapack(),
        |_| {},
        &["--yes"],
        StdinConnection::Terminal(b"n\n".to_vec()),
    );

    assert!(
        !output
            .stdout
            .contains(&question(&environment, "Example.zip")),
        "stdout: {}",
        output.stdout
    );
}

/// `mism datapack install` of the local `Example.zip` without `--yes`, with `y\n` on a closed
/// pipe.
fn install_local_example_from_a_pipe() -> (DatapackEnvironment, Output, WorldState) {
    install_local_example(
        &example_datapack(),
        |_| {},
        &[],
        StdinConnection::ClosedAfter(b"y\n".to_vec()),
    )
}

/// dinst-7: stdin that is not a terminal, without `--yes`, is an error.
#[test]
fn stdin_that_is_not_a_terminal_is_an_error() {
    let (_environment, output, _before) = install_local_example_from_a_pipe();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinst-7: mism does not ask.
#[test]
fn stdin_that_is_not_a_terminal_is_not_asked() {
    let (environment, output, _before) = install_local_example_from_a_pipe();

    assert!(
        !output
            .stdout
            .contains(&question(&environment, "Example.zip")),
        "stdout: {}",
        output.stdout
    );
}

/// dinst-7, dp-11: the error changes nothing.
#[test]
fn stdin_that_is_not_a_terminal_changes_nothing() {
    let (environment, _output, before) = install_local_example_from_a_pipe();

    assert_eq!(environment.world_state(), before);
}

/// d-install-1: no request to Modrinth.
#[test]
fn stdin_that_is_not_a_terminal_sends_no_request_to_modrinth() {
    let environment = DatapackEnvironment::with_server("paper", "26.1.2");
    environment
        .modrinth
        .serve_true_ending(&true_ending_file(TRUE_ENDING_FILE_NAME));

    install(
        &environment,
        TRUE_ENDING_SOURCE,
        &[],
        StdinConnection::ClosedAfter(b"y\n".to_vec()),
    );

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// d-install-1: no request to the URL.
#[test]
fn stdin_that_is_not_a_terminal_sends_no_request_to_the_url() {
    let (environment, _output) =
        install_example_from_a_url(&[], StdinConnection::ClosedAfter(b"\ny\n".to_vec()));

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// d-install-1: the cache directory stays as it was, which is absent here.
#[test]
fn stdin_that_is_not_a_terminal_leaves_the_cache_as_it_was() {
    let (environment, _output, _before) = install_local_example_from_a_pipe();

    assert!(!environment.cache().exists());
}

/// dp-10, dplace-5: the installation ends with its record and the notice.
#[test]
fn installation_ends_with_its_record_and_the_notice() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_datapack(), |_| {});

    assert!(
        output.stdout.ends_with(&format!(
            "\nmism: Example\t-\tExample.zip\nmism: {TAKES_EFFECT_NOTICE}\n"
        )),
        "stdout: {}",
        output.stdout
    );
}

/// d-pcmd-2: SIGINT at the question leaves the world directory and `mism.toml` as they were.
#[test]
fn interruption_at_the_question_leaves_the_world_as_it_was() {
    let (environment, _output, before) = signal_at_the_confirmation(Signal::SIGINT);

    assert_eq!(environment.world_state(), before);
}

/// d-pcmd-2: SIGTERM at the question leaves the world directory and `mism.toml` as they were.
#[test]
fn termination_at_the_question_leaves_the_world_as_it_was() {
    let (environment, _output, before) = signal_at_the_confirmation(Signal::SIGTERM);

    assert_eq!(environment.world_state(), before);
}

/// 「コマンドラインとエラーの出力」 方法: SIGTERM at the question ends mism with status 143.
#[test]
fn termination_at_the_question_exits_with_143() {
    let (_environment, output, _before) = signal_at_the_confirmation(Signal::SIGTERM);

    assert_eq!(output.exit_code, 143);
}
