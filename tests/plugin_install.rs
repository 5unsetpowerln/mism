mod common;

use std::fs;
use std::path::Path;

use nix::sys::signal::Signal;

use common::fake_modrinth::{LUCKPERMS_FILE_NAME, LUCKPERMS_SOURCE, ServedFile, sha256_hex};
use common::mism_process::Output;
use common::plugin_output::{described, has_line, last_record, line_index};
use common::plugin_server_directory::{PluginEnvironment, Record, ServerState, mism_toml_text};
use common::stdin_connection::StdinConnection;
use common::test_plugin_jar::{jar_with_plugin_yml, jar_without_declaration, plugin_yml};

const NO_TARGET_SERVER: &str =
    "the current directory has no mism.toml, so it is not a server directory";
const TAKES_EFFECT_NOTICE: &str = "the change takes effect when the server starts next";
const EXAMPLE_PATH: &str = "/downloads/Example.jar";
const QUESTION: &str = "place Example.jar in plugins/? (y/n): ";
const NOT_PLACING: &str = "not placing Example.jar";
const NOT_A_TERMINAL: &str = "stdin is not a terminal, so mism cannot ask whether to place the jar";
const FAWE: &str = "fastasyncworldedit";
const FAWE_PAPER_2_16_1: &str = "FastAsyncWorldEdit-Paper-2.16.1.jar";
const FAWE_PAPER_2_16_0: &str = "FastAsyncWorldEdit-Paper-2.16.0.jar";
const FAWE_PAPER_2_12_2: &str = "FastAsyncWorldEdit-Paper-2.12.2.jar";
/// The source of the file `--lazy` chooses on a Paper server for 26.3 (d-select-1).
const FAWE_LAZY_SOURCE: &str = "modrinth:fastasyncworldedit:2.16.1:paper";

fn example_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("Example", "1.0", "1.21"))
}

fn luckperms_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("LuckPerms", "5.5.71", "1.13"))
}

/// The jar of every FastAsyncWorldEdit file served, with an api-version that no server of the
/// tests is older than.
fn fawe_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("FastAsyncWorldEdit", "2.16.1", "1.13"))
}

/// A jar of another plugin, which conflicts with nothing installed here.
fn other_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("Other", "1.0", "1.13"))
}

/// A server of `software` for `minecraft_version` whose fake Modrinth serves FastAsyncWorldEdit,
/// with the files the tests install served.
fn serving_fastasyncworldedit(software: &str, minecraft_version: &str) -> PluginEnvironment {
    let environment = PluginEnvironment::with_server(software, minecraft_version);
    let jar = fawe_jar();
    environment.modrinth.mount_project(FAWE);
    environment.modrinth.mount_version_list(
        FAWE,
        &[
            ServedFile::new(FAWE_PAPER_2_16_1, &jar),
            ServedFile::new(FAWE_PAPER_2_16_0, &jar),
            ServedFile::new(FAWE_PAPER_2_12_2, &jar),
        ],
    );
    environment
}

/// `mism plugin install <source> <extra>` in `environment` with stdin connected as `stdin`.
fn install(
    environment: &PluginEnvironment,
    source: &str,
    extra: &[&str],
    stdin: StdinConnection,
) -> Output {
    environment
        .mism_plugin(["install", source])
        .arguments(extra)
        .stdin(stdin)
        .run()
}

/// `mism plugin install <source> <extra>` of FastAsyncWorldEdit on a server of `software` for
/// `minecraft_version`.
fn install_fastasyncworldedit(
    software: &str,
    minecraft_version: &str,
    source: &str,
    extra: &[&str],
    stdin: StdinConnection,
) -> (PluginEnvironment, Output) {
    let environment = serving_fastasyncworldedit(software, minecraft_version);
    let output = install(&environment, source, extra, stdin);
    (environment, output)
}

/// `mism plugin install modrinth:fastasyncworldedit --lazy --yes` on a Paper server for 26.3.
fn install_fastasyncworldedit_lazily() -> (PluginEnvironment, Output) {
    install_fastasyncworldedit(
        "paper",
        "26.3",
        "modrinth:fastasyncworldedit",
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    )
}

/// `mism plugin install modrinth:fastasyncworldedit --lazy --yes` on a Folia server for 26.3,
/// which no version of FastAsyncWorldEdit matches.
fn install_fastasyncworldedit_lazily_on_folia() -> (PluginEnvironment, Output) {
    install_fastasyncworldedit(
        "folia",
        "26.3",
        "modrinth:fastasyncworldedit",
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    )
}

/// Installs the local `Example.jar` with `bytes` on a Paper server for 26.3 prepared by
/// `prepare`, with the further arguments `extra` and stdin connected as `stdin`. Returns the
/// server's state before the installation too.
fn install_local_example(
    bytes: &[u8],
    prepare: impl FnOnce(&PluginEnvironment),
    extra: &[&str],
    stdin: StdinConnection,
) -> (PluginEnvironment, Output, ServerState) {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    prepare(&environment);
    let jar = environment.write_local_jar("Example.jar", bytes);
    let before = environment.server_state();
    let output = install(&environment, jar.to_str().unwrap(), extra, stdin);
    (environment, output, before)
}

/// Installs the local `Example.jar` with `bytes` on a server prepared by `prepare`, with `--yes`.
fn install_local_example_with_yes(
    bytes: &[u8],
    prepare: impl FnOnce(&PluginEnvironment),
) -> (PluginEnvironment, Output, ServerState) {
    install_local_example(bytes, prepare, &["--yes"], StdinConnection::NeverClosing)
}

/// Installs the local `Example.jar`, answering the confirmation with `typed` on a terminal.
fn install_local_example_answering(typed: &[u8]) -> (PluginEnvironment, Output, ServerState) {
    install_local_example(
        &example_jar(),
        |_| {},
        &[],
        StdinConnection::Terminal(typed.to_vec()),
    )
}

/// Installs `Example.jar` from a URL on a Paper server for 26.3, with the further arguments
/// `extra` and stdin connected as `stdin`.
fn install_example_from_a_url(
    extra: &[&str],
    stdin: StdinConnection,
) -> (PluginEnvironment, Output) {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &example_jar());
    let output = install(&environment, &url, extra, stdin);
    (environment, output)
}

fn placed(environment: &PluginEnvironment, file_name: &str) -> Option<Vec<u8>> {
    fs::read(environment.plugins().join(file_name)).ok()
}

/// A jar in `plugins/` that declares the plugin name `Example`.
fn put_conflicting_jar(environment: &PluginEnvironment) {
    environment.put_in_plugins("Old.jar", &example_jar());
}

/// `mism.toml` with a record of `Example` whose jar is `Old.jar`.
fn record_named_example(environment: &PluginEnvironment) {
    environment.write_mism_toml(&mism_toml_text(
        "paper",
        "26.3",
        &[Record {
            name: "Example",
            source: "https://example.com/Old.jar",
            file_name: "Old.jar",
            sha256: &sha256_hex(b"Old"),
        }],
    ));
}

/// A jar named `Example.jar` in `plugins/`, of another plugin.
fn put_jar_named_example(environment: &PluginEnvironment) {
    environment.put_in_plugins("Example.jar", &other_jar());
}

/// Installs the local `Example.jar` with nothing typed on its terminal, sends `signal` once the
/// confirmation is asked, and returns the output and the server's state before the run.
fn signal_at_the_confirmation(signal: Signal) -> (PluginEnvironment, Output, ServerState) {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.put_in_plugins("Other.jar", &other_jar());
    let jar = environment.write_local_jar("Example.jar", &example_jar());
    let before = environment.server_state();
    let mut mism = environment
        .mism_plugin(["install".as_ref(), jar.as_os_str()])
        .stdin(StdinConnection::Terminal(Vec::new()))
        .spawn();
    mism.wait_for_stdout(&format!("mism: {QUESTION}"));

    mism.send(signal);
    let output = mism.wait();
    (environment, output, before)
}

/// plg-1: `mism plugin install` without `mism.toml` is an error.
#[test]
fn install_without_mism_toml_is_an_error() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = install(
        &environment,
        LUCKPERMS_SOURCE,
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert!(has_line(&output.stdout, NO_TARGET_SERVER));
}

/// plg-1: without `mism.toml`, `mism plugin install` sends no request.
#[test]
fn install_without_mism_toml_sends_no_request() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    install(
        &environment,
        LUCKPERMS_SOURCE,
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// inst-1: the file a `modrinth:` source names is installed.
#[test]
fn modrinth_file_is_installed() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.modrinth.serve_luckperms(&luckperms_jar());

    install(
        &environment,
        LUCKPERMS_SOURCE,
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(
        placed(&environment, LUCKPERMS_FILE_NAME),
        Some(luckperms_jar())
    );
}

/// inst-1: without `--lazy`, the files are narrowed as plg-10 and plg-11 say, by the answers on
/// a terminal.
#[test]
fn modrinth_file_is_chosen_by_the_answers() {
    let (environment, _output) = install_fastasyncworldedit(
        "paper",
        "26.3",
        "modrinth:fastasyncworldedit:2.16.1",
        &[],
        StdinConnection::Terminal(b"paper\ny\n".to_vec()),
    );

    assert_eq!(placed(&environment, FAWE_PAPER_2_16_1), Some(fawe_jar()));
}

/// inst-1: the jar of a URL is installed.
#[test]
fn url_jar_is_installed() {
    let (environment, _output) =
        install_example_from_a_url(&["--yes"], StdinConnection::NeverClosing);

    assert_eq!(placed(&environment, "Example.jar"), Some(example_jar()));
}

/// inst-1: a local jar file is installed.
#[test]
fn local_jar_is_installed() {
    let (environment, _output, _before) = install_local_example_with_yes(&example_jar(), |_| {});

    assert_eq!(placed(&environment, "Example.jar"), Some(example_jar()));
}

/// inst-2: `--lazy` installs the newest version that matches the server.
#[test]
fn lazy_installs_the_newest_version_that_matches_the_server() {
    let (environment, _output) = install_fastasyncworldedit_lazily();

    assert_eq!(placed(&environment, FAWE_PAPER_2_16_1), Some(fawe_jar()));
}

/// inst-2: after `--lazy` chose the version, the files are narrowed as plg-10 and plg-11 say.
/// On Paper 1.19.4 the newest matching version, 2.12.2, has two files.
#[test]
fn lazy_asks_for_the_attributes_after_the_version() {
    let (environment, _output) = install_fastasyncworldedit(
        "paper",
        "1.19.4",
        "modrinth:fastasyncworldedit",
        &["--lazy"],
        StdinConnection::Terminal(format!("{FAWE_PAPER_2_12_2}\ny\n").into_bytes()),
    );

    assert_eq!(placed(&environment, FAWE_PAPER_2_12_2), Some(fawe_jar()));
}

/// inst-2: without a matching version, `--lazy` is an error.
#[test]
fn lazy_without_a_matching_version_is_an_error() {
    let (_environment, output) = install_fastasyncworldedit_lazily_on_folia();

    assert!(has_line(
        &output.stdout,
        "no version of fastasyncworldedit matches the server (folia 26.3)"
    ));
}

/// inst-2: the error tells to look at the versions and give a version number.
#[test]
fn lazy_without_a_matching_version_hints_at_the_versions() {
    let (_environment, output) = install_fastasyncworldedit_lazily_on_folia();

    assert!(has_line(
        &output.stdout,
        "hint: run `mism plugin versions modrinth:fastasyncworldedit` to see how each version \
         matches the server, and give a source with a version number"
    ));
}

/// inst-2: without a matching version, mism chooses no other version, so it downloads nothing.
#[test]
fn lazy_without_a_matching_version_downloads_nothing() {
    let (environment, _output) = install_fastasyncworldedit_lazily_on_folia();

    let downloads: Vec<String> = environment
        .modrinth
        .request_paths()
        .into_iter()
        .filter(|path| path.starts_with("/files/"))
        .collect();
    assert_eq!(downloads, Vec::<String>::new());
}

/// d-select-1: the source of the file `--lazy` chose names only that file.
#[test]
fn lazy_choice_has_the_source_of_its_file() {
    let (_environment, output) = install_fastasyncworldedit_lazily();

    assert_eq!(
        described(&output.stdout, "source"),
        Some(format!("source\t{FAWE_LAZY_SOURCE}").as_str())
    );
}

/// d-select-1: the source printed for the file `--lazy` chose finds the same file without
/// `--lazy` and without asking.
#[test]
fn source_of_a_lazy_choice_names_the_same_file_without_lazy() {
    let (_environment, lazy) = install_fastasyncworldedit_lazily();
    let printed_source = described(&lazy.stdout, "source").unwrap()["source\t".len()..].to_owned();
    let environment = serving_fastasyncworldedit("paper", "26.3");

    let output = environment
        .mism_plugin(["fetch", &printed_source])
        .stdin(StdinConnection::NeverClosing)
        .run();

    let cached = last_record(&output.stdout)[4].to_owned();
    assert_eq!(
        Path::new(&cached).file_name().unwrap().to_str(),
        Some(FAWE_PAPER_2_16_1)
    );
}

/// d-select-1: the record in `mism.toml` has the source of the file `--lazy` chose.
#[test]
fn lazy_choice_is_recorded_with_the_source_of_its_file() {
    let (environment, _output) = install_fastasyncworldedit_lazily();

    assert_eq!(
        environment.read_mism_toml(),
        mism_toml_text(
            "paper",
            "26.3",
            &[Record {
                name: "FastAsyncWorldEdit",
                source: FAWE_LAZY_SOURCE,
                file_name: FAWE_PAPER_2_16_1,
                sha256: &sha256_hex(&fawe_jar()),
            }]
        )
    );
}

/// inst-3: `--lazy` with a URL is ignored.
#[test]
fn lazy_is_ignored_with_a_url() {
    let (environment, _output) =
        install_example_from_a_url(&["--lazy", "--yes"], StdinConnection::NeverClosing);

    assert_eq!(placed(&environment, "Example.jar"), Some(example_jar()));
}

/// inst-3: `--lazy` with a local jar file is ignored.
#[test]
fn lazy_is_ignored_with_a_local_jar() {
    let (environment, _output, _before) = install_local_example(
        &example_jar(),
        |_| {},
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(placed(&environment, "Example.jar"), Some(example_jar()));
}

/// inst-3: `--lazy` with a version number is ignored: the version given is installed, not the
/// newest matching one.
#[test]
fn lazy_is_ignored_with_a_version_number() {
    let (environment, _output) = install_fastasyncworldedit(
        "paper",
        "26.3",
        "modrinth:fastasyncworldedit:2.16.0:paper",
        &["--lazy", "--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(placed(&environment, FAWE_PAPER_2_16_0), Some(fawe_jar()));
}

/// inst-4: the output is the fetch, the inspection, the confirmation, and the placement, in this
/// order.
#[test]
fn output_is_fetch_inspection_confirmation_and_placement() {
    let (_environment, output) =
        install_example_from_a_url(&[], StdinConnection::Terminal(b"\ny\n".to_vec()));

    let indices: Vec<Option<usize>> = [
        "plugin name from the URL: Example",
        "name\tExample",
        "declared\tunknown",
        QUESTION,
        "Example\t-\tExample.jar",
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

/// inst-4, plg-4: after the installation, `mism.toml` has the record of the plugin.
#[test]
fn installed_plugin_is_recorded() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let jar = environment.write_local_jar("Example.jar", &example_jar());
    let source = jar.to_str().unwrap();

    install(
        &environment,
        source,
        &["--yes"],
        StdinConnection::NeverClosing,
    );

    assert_eq!(
        environment.read_mism_toml(),
        mism_toml_text(
            "paper",
            "26.3",
            &[Record {
                name: "Example",
                source,
                file_name: "Example.jar",
                sha256: &sha256_hex(&example_jar()),
            }]
        )
    );
}

/// inst-4: `incompatible` stops the installation after the inspection.
#[test]
fn incompatible_stops_the_installation_after_the_inspection() {
    let (_environment, output, _before) = install_local_example_with_yes(
        &jar_with_plugin_yml(&plugin_yml("Example", "1.0", "26.4")),
        |_| {},
    );

    let indices = [
        line_index(&output.stdout, "name\tExample"),
        line_index(
            &output.stdout,
            "the inspection found `incompatible`, so Example.jar is not placed",
        ),
    ];
    assert!(
        indices.iter().all(Option::is_some) && indices.is_sorted(),
        "stdout: {}",
        output.stdout
    );
}

/// inst-4: `incompatible` changes nothing.
#[test]
fn incompatible_changes_nothing() {
    let (environment, _output, before) = install_local_example_with_yes(
        &jar_with_plugin_yml(&plugin_yml("Example", "1.0", "26.4")),
        |_| {},
    );

    assert_eq!(environment.server_state(), before);
}

/// inst-4: `conflict` stops the installation.
#[test]
fn conflict_stops_the_installation() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_jar(), put_conflicting_jar);

    assert!(has_line(
        &output.stdout,
        "the inspection found `conflict`, so Example.jar is not placed"
    ));
}

/// inst-4: `conflict` changes nothing.
#[test]
fn conflict_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&example_jar(), put_conflicting_jar);

    assert_eq!(environment.server_state(), before);
}

/// inst-4: a jar without a declaration is an error as insp-4 says.
#[test]
fn jar_without_a_declaration_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&jar_without_declaration(), |_| {});

    assert!(
        output
            .stdout
            .ends_with(" has neither plugin.yml nor paper-plugin.yml, so it declares no plugin\n")
    );
}

/// inst-4: a jar without a declaration changes nothing.
#[test]
fn jar_without_a_declaration_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&jar_without_declaration(), |_| {});

    assert_eq!(environment.server_state(), before);
}

/// inst-4, place-3: a record with the same name is an error.
#[test]
fn record_with_the_same_name_is_an_error() {
    let (_environment, output, _before) =
        install_local_example_with_yes(&example_jar(), record_named_example);

    assert!(has_line(
        &output.stdout,
        "mism.toml already has a record of the plugin Example"
    ));
}

/// inst-4, place-3: a record with the same name changes nothing.
#[test]
fn record_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&example_jar(), record_named_example);

    assert_eq!(environment.server_state(), before);
}

/// inst-4, place-3 (「mism plugin install と mism datapack install の手順」 方法 6): a record with
/// the same name is found before the confirmation is asked.
#[test]
fn record_with_the_same_name_is_found_before_asking() {
    let (_environment, output, _before) = install_local_example(
        &example_jar(),
        record_named_example,
        &[],
        StdinConnection::Terminal(b"y\n".to_vec()),
    );

    assert!(
        !output.stdout.contains(QUESTION),
        "stdout: {}",
        output.stdout
    );
}

/// inst-4, place-4: a jar with the same name in `plugins/` is an error.
#[test]
fn jar_with_the_same_name_is_an_error() {
    let (environment, output, _before) =
        install_local_example_with_yes(&example_jar(), put_jar_named_example);

    assert!(has_line(
        &output.stdout,
        &format!(
            "{} already exists",
            environment.plugins().join("Example.jar").display()
        )
    ));
}

/// inst-4, place-4: a jar with the same name in `plugins/` changes nothing.
#[test]
fn jar_with_the_same_name_changes_nothing() {
    let (environment, _output, before) =
        install_local_example_with_yes(&example_jar(), put_jar_named_example);

    assert_eq!(environment.server_state(), before);
}

/// inst-5: without `--yes`, mism asks whether to place the jar.
#[test]
fn confirmation_is_asked() {
    let (_environment, output, _before) = install_local_example_answering(b"n\n");

    assert!(
        has_line(&output.stdout, QUESTION),
        "stdout: {}",
        output.stdout
    );
}

/// inst-5: the answer `y` places the jar.
#[test]
fn answer_y_places_the_jar() {
    let (environment, _output, _before) = install_local_example_answering(b"y\n");

    assert_eq!(placed(&environment, "Example.jar"), Some(example_jar()));
}

/// inst-5: the answer `yes` places the jar.
#[test]
fn answer_yes_places_the_jar() {
    let (environment, _output, _before) = install_local_example_answering(b"yes\n");

    assert_eq!(placed(&environment, "Example.jar"), Some(example_jar()));
}

/// inst-5: another answer writes that the jar is not placed.
#[test]
fn other_answer_writes_that_the_jar_is_not_placed() {
    let (_environment, output, _before) = install_local_example_answering(b"n\n");

    assert!(has_line(&output.stdout, NOT_PLACING));
}

/// inst-5: another answer changes nothing.
#[test]
fn other_answer_changes_nothing() {
    let (environment, _output, before) = install_local_example_answering(b"n\n");

    assert_eq!(environment.server_state(), before);
}

/// inst-5: another answer exits with status 0.
#[test]
fn other_answer_exits_with_status_0() {
    let (_environment, output, _before) = install_local_example_answering(b"n\n");

    assert_eq!(output.exit_code, 0);
}

/// inst-5: an empty answer is another answer.
#[test]
fn empty_answer_does_not_place_the_jar() {
    let (_environment, output, _before) = install_local_example_answering(b"\n");

    assert!(has_line(&output.stdout, NOT_PLACING));
}

/// inst-5: `Y` is not `y`.
#[test]
fn capital_y_does_not_place_the_jar() {
    let (_environment, output, _before) = install_local_example_answering(b"Y\n");

    assert!(has_line(&output.stdout, NOT_PLACING));
}

/// inst-6: with `--yes`, mism places the jar without asking, whatever is typed.
#[test]
fn yes_places_the_jar_without_asking() {
    let (environment, _output, _before) = install_local_example(
        &example_jar(),
        |_| {},
        &["--yes"],
        StdinConnection::Terminal(b"n\n".to_vec()),
    );

    assert_eq!(placed(&environment, "Example.jar"), Some(example_jar()));
}

/// inst-6: with `--yes`, there is no confirmation.
#[test]
fn yes_writes_no_confirmation() {
    let (_environment, output, _before) = install_local_example(
        &example_jar(),
        |_| {},
        &["--yes"],
        StdinConnection::Terminal(b"n\n".to_vec()),
    );

    assert!(
        !output.stdout.contains(QUESTION),
        "stdout: {}",
        output.stdout
    );
}

/// `mism plugin install` of the local `Example.jar` without `--yes`, with `y\n` on a closed pipe.
fn install_local_example_from_a_pipe() -> (PluginEnvironment, Output, ServerState) {
    install_local_example(
        &example_jar(),
        |_| {},
        &[],
        StdinConnection::ClosedAfter(b"y\n".to_vec()),
    )
}

/// inst-7: stdin that is not a terminal, without `--yes`, is an error.
#[test]
fn stdin_that_is_not_a_terminal_is_an_error() {
    let (_environment, output, _before) = install_local_example_from_a_pipe();

    assert!(has_line(&output.stdout, NOT_A_TERMINAL));
}

/// inst-7: the error tells to add `--yes`.
#[test]
fn stdin_that_is_not_a_terminal_hints_at_yes() {
    let (_environment, output, _before) = install_local_example_from_a_pipe();

    assert!(has_line(
        &output.stdout,
        "hint: add --yes to place the jar without asking"
    ));
}

/// inst-7: the error exits with status 1 (err-9).
#[test]
fn stdin_that_is_not_a_terminal_exits_with_status_1() {
    let (_environment, output, _before) = install_local_example_from_a_pipe();

    assert_eq!(output.exit_code, 1);
}

/// inst-7: mism does not ask.
#[test]
fn stdin_that_is_not_a_terminal_is_not_asked() {
    let (_environment, output, _before) = install_local_example_from_a_pipe();

    assert!(
        !output.stdout.contains(QUESTION),
        "stdout: {}",
        output.stdout
    );
}

/// inst-7, plg-8: the error changes nothing.
#[test]
fn stdin_that_is_not_a_terminal_changes_nothing() {
    let (environment, _output, before) = install_local_example_from_a_pipe();

    assert_eq!(environment.server_state(), before);
}

/// d-install-1: no request to Modrinth.
#[test]
fn stdin_that_is_not_a_terminal_sends_no_request_to_modrinth() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.modrinth.serve_luckperms(&luckperms_jar());

    install(
        &environment,
        LUCKPERMS_SOURCE,
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

/// plg-7, place-5: the installation ends with its record and the notice.
#[test]
fn installation_ends_with_its_record_and_the_notice() {
    let (_environment, output, _before) = install_local_example_with_yes(&example_jar(), |_| {});

    assert!(
        output.stdout.ends_with(&format!(
            "\nmism: Example\t-\tExample.jar\nmism: {TAKES_EFFECT_NOTICE}\n"
        )),
        "stdout: {}",
        output.stdout
    );
}

/// inst-4: a successful installation exits with status 0.
#[test]
fn successful_installation_exits_with_status_0() {
    let (_environment, output, _before) = install_local_example_with_yes(&example_jar(), |_| {});

    assert_eq!(output.exit_code, 0);
}

/// d-pcmd-1: SIGINT at the confirmation leaves `plugins/` and `mism.toml` as they were.
#[test]
fn interruption_at_the_confirmation_leaves_the_server_as_it_was() {
    let (environment, _output, before) = signal_at_the_confirmation(Signal::SIGINT);

    assert_eq!(environment.server_state(), before);
}

/// d-pcmd-1: SIGTERM at the confirmation leaves `plugins/` and `mism.toml` as they were.
#[test]
fn termination_at_the_confirmation_leaves_the_server_as_it_was() {
    let (environment, _output, before) = signal_at_the_confirmation(Signal::SIGTERM);

    assert_eq!(environment.server_state(), before);
}

/// d-prompt-1: SIGINT at the confirmation ends mism with status 130.
#[test]
fn interruption_at_the_confirmation_exits_with_130() {
    let (_environment, output, _before) = signal_at_the_confirmation(Signal::SIGINT);

    assert_eq!(output.exit_code, 130);
}
