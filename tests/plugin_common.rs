mod common;

use std::path::Path;
use std::time::Duration;

use nix::sys::signal::Signal;
use serde_json::Value;

use common::fake_modrinth::{
    LUCKPERMS_FILE_NAME, LUCKPERMS_SOURCE, ServedFile, saved_project, saved_version_list,
};
use common::mism_process::Output;
use common::plugin_output::{described, has_line, last_record};
use common::plugin_server_directory::PluginEnvironment;
use common::pseudo_terminal;
use common::stdin_connection::StdinConnection;
use common::test_plugin_jar::{jar_with_plugin_yml, plugin_yml};

const NO_TARGET_SERVER: &str =
    "the current directory has no mism.toml, so it is not a server directory";
const TAKES_EFFECT_NOTICE: &str = "the change takes effect when the server starts next";
const FAWE: &str = "fastasyncworldedit";
const FAWE_PAPER_2_16_1: &str = "FastAsyncWorldEdit-Paper-2.16.1.jar";

fn example_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("Example", "1.0", "1.21"))
}

fn luckperms_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("LuckPerms", "5.5.71", "1.13"))
}

/// The file name at the end of the cached jar's path in a fetch-5 record.
fn fetched_file_name(output: &Output) -> String {
    let record = last_record(&output.stdout);
    Path::new(record[4])
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

/// An environment whose fake Modrinth serves FastAsyncWorldEdit, with the files of 2.16.1 and
/// 2.14.1 served.
fn serving_fastasyncworldedit(environment: PluginEnvironment) -> PluginEnvironment {
    let jar = example_jar();
    environment.modrinth.mount_project(FAWE);
    environment.modrinth.mount_version_list(
        FAWE,
        &[
            ServedFile::new(FAWE_PAPER_2_16_1, &jar),
            ServedFile::new("FastAsyncWorldEdit-Bukkit-2.16.1.jar", &jar),
            ServedFile::new("FastAsyncWorldEdit-Paper-2.14.1.jar", &jar),
        ],
    );
    environment
}

/// `mism plugin fetch <source>` from the served FastAsyncWorldEdit, with stdin connected as
/// `stdin` and the further arguments `extra`.
fn fetch_fastasyncworldedit(source: &str, stdin: StdinConnection, extra: &[&str]) -> Output {
    let environment = serving_fastasyncworldedit(PluginEnvironment::without_server());
    environment
        .mism_plugin(["fetch", source])
        .arguments(extra)
        .stdin(stdin)
        .run()
}

/// `mism plugin fetch <url> --yes` of `Example.jar` served at `url_path`.
fn fetch_from(url_path: &str) -> Output {
    let environment = PluginEnvironment::without_server();
    let url = environment.modrinth.mount_jar(url_path, &example_jar());
    environment.fetch(&url)
}

/// plg-1: `mism plugin place` without `mism.toml`.
#[test]
fn place_without_mism_toml_is_an_error() {
    let environment = PluginEnvironment::without_server();

    let output = environment.mism_plugin(["place", LUCKPERMS_SOURCE]).run();

    assert!(has_line(&output.stdout, NO_TARGET_SERVER));
}

/// plg-1: the error tells where to run the command.
#[test]
fn missing_mism_toml_hints_at_a_server_directory() {
    let environment = PluginEnvironment::without_server();

    let output = environment.mism_plugin(["place", LUCKPERMS_SOURCE]).run();

    assert!(has_line(
        &output.stdout,
        "hint: run the command in a server directory that `mism new` created"
    ));
}

/// plg-1: `mism plugin remove` without `mism.toml`.
#[test]
fn remove_without_mism_toml_is_an_error() {
    let environment = PluginEnvironment::without_server();

    let output = environment.mism_plugin(["remove", "LuckPerms"]).run();

    assert!(has_line(&output.stdout, NO_TARGET_SERVER));
}

/// plg-1: `mism plugin list` without `--global` and without `mism.toml`.
#[test]
fn list_without_mism_toml_is_an_error() {
    let environment = PluginEnvironment::without_server();

    let output = environment.mism_plugin(["list"]).run();

    assert!(has_line(&output.stdout, NO_TARGET_SERVER));
}

/// plg-2: `mism plugin inspect` without `mism.toml` goes on without a target server.
#[test]
fn inspect_without_mism_toml_succeeds() {
    let environment = PluginEnvironment::without_server();
    let jar = environment.write_local_jar("Example.jar", &example_jar());

    let output = environment
        .mism_plugin(["inspect".as_ref(), jar.as_os_str()])
        .run();

    assert_eq!(output.exit_code, 0);
}

/// plg-2: without a target server there are no judgements.
#[test]
fn inspect_without_mism_toml_judges_nothing() {
    let environment = PluginEnvironment::without_server();
    let jar = environment.write_local_jar("Example.jar", &example_jar());

    let output = environment
        .mism_plugin(["inspect".as_ref(), jar.as_os_str()])
        .run();

    assert_eq!(described(&output.stdout, "declared"), None);
}

/// plg-2: with `mism.toml` the current directory is the target server.
#[test]
fn inspect_with_mism_toml_judges_against_the_server() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let jar = environment.write_local_jar("Example.jar", &example_jar());

    let output = environment
        .mism_plugin(["inspect".as_ref(), jar.as_os_str()])
        .run();

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tunknown")
    );
}

/// plg-3: a `modrinth:` source with values is a source.
#[test]
fn modrinth_source_with_values_is_fetched() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1:paper",
        StdinConnection::NeverClosing,
        &[],
    );

    assert_eq!(output.exit_code, 0);
}

/// plg-3: a URL starting with `http://` is a source.
#[test]
fn url_is_fetched() {
    let output = fetch_from("/downloads/Example.jar");

    assert_eq!(output.exit_code, 0);
}

/// plg-3: the path of a local jar file is a source.
#[test]
fn local_jar_is_fetched() {
    let environment = PluginEnvironment::without_server();
    let jar = environment.write_local_jar("Example.jar", &example_jar());

    let output = environment.fetch(jar.to_str().unwrap());

    assert_eq!(output.exit_code, 0);
}

/// plg-3: a string of none of the three forms is an error that shows the three forms.
#[test]
fn string_of_no_form_is_an_error_with_the_three_forms() {
    let environment = PluginEnvironment::without_server();

    let output = environment.fetch("LuckPerms");

    assert!(has_line(
        &output.stdout,
        "hint: give `modrinth:<identifier>` (optionally followed by `:<value>`s), a jar URL \
         starting with `https://` or `http://`, or the path of a local jar file"
    ));
}

/// plg-3: an identifier shorter than 3 characters is not an identifier.
#[test]
fn short_identifier_is_an_error() {
    let environment = PluginEnvironment::without_server();

    let output = environment.fetch("modrinth:lp");

    assert!(
        output
            .stdout
            .starts_with("mism: \"lp\" is not a Modrinth identifier")
    );
}

/// plg-3: an identifier is the slug, so one that differs from it in case is an error.
#[test]
fn identifier_differing_from_the_slug_in_case_is_an_error() {
    let environment = PluginEnvironment::without_server();
    environment
        .modrinth
        .mount_project_json("LuckPerms", &saved_project("luckperms"));

    let output = environment.fetch("modrinth:LuckPerms");

    assert!(has_line(
        &output.stdout,
        "hint: use the identifier in the source: `modrinth:luckperms`"
    ));
}

/// plg-3: an identifier of no Modrinth project is an error.
#[test]
fn identifier_of_no_project_is_an_error() {
    let environment = PluginEnvironment::without_server();

    let output = environment.fetch("modrinth:no-such-plugin");

    assert!(has_line(
        &output.stdout,
        "Modrinth has no project with the identifier \"no-such-plugin\""
    ));
}

/// plg-5: a plugin from Modrinth is named by its name on Modrinth.
#[test]
fn modrinth_plugin_name_is_its_title() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert_eq!(last_record(&output.stdout)[0], "LuckPerms");
}

/// plg-5: a plugin from a URL is named as fetch-2 decides.
#[test]
fn url_plugin_name_is_made_from_the_url() {
    let output = fetch_from("/downloads/Example.jar");

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// plg-5: a local jar file is named by its file name without `.jar`.
#[test]
fn local_plugin_name_is_the_file_name_without_jar() {
    let environment = PluginEnvironment::without_server();
    let jar = environment.write_local_jar("My.Plugin.jar", &example_jar());

    let output = environment.fetch(jar.to_str().unwrap());

    assert_eq!(last_record(&output.stdout)[0], "My.Plugin");
}

/// plg-6: an empty plugin name is an error.
#[test]
fn empty_plugin_name_is_an_error() {
    let output = fetch_from("/downloads/.jar");

    assert!(
        output
            .stdout
            .contains("mism: the plugin name \"\" cannot be used")
    );
}

/// plg-6: the plugin name `.` is an error.
#[test]
fn dot_plugin_name_is_an_error() {
    let output = fetch_from("/downloads/..jar");

    assert!(
        output
            .stdout
            .contains("mism: the plugin name \".\" cannot be used")
    );
}

/// plg-6: the plugin name `..` is an error.
#[test]
fn dot_dot_plugin_name_is_an_error() {
    let output = fetch_from("/downloads/...jar");

    assert!(
        output
            .stdout
            .contains("mism: the plugin name \"..\" cannot be used")
    );
}

/// plg-7: `mism plugin place` ends with the notice.
#[test]
fn place_ends_with_the_notice() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.modrinth.serve_luckperms(&luckperms_jar());
    environment.fetch(LUCKPERMS_SOURCE);

    let output = environment.mism_plugin(["place", LUCKPERMS_SOURCE]).run();

    assert_eq!(
        output.stdout.lines().last(),
        Some(format!("mism: {TAKES_EFFECT_NOTICE}").as_str())
    );
}

/// plg-9: the values of a record are separated by tabs, and an absent value is `-` (the version
/// number of a URL source).
#[test]
fn record_values_are_separated_by_tabs_with_a_hyphen_for_none() {
    let output = fetch_from("/downloads/Example.jar");

    assert_eq!(last_record(&output.stdout)[1], "-");
}

/// plg-9: the elements of a list value are separated by `,`.
#[test]
fn list_elements_are_separated_by_commas() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.mism_plugin(["inspect", LUCKPERMS_SOURCE]).run();

    assert_eq!(
        described(&output.stdout, "compatibility.software"),
        Some("compatibility.software\tbukkit,folia,paper,spigot\tmodrinth")
    );
}

/// plg-10: the first value given is the version number.
#[test]
fn first_value_is_the_version_number() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert_eq!(fetched_file_name(&output), LUCKPERMS_FILE_NAME);
}

/// plg-10: without values, the only version number of the candidates is the version number,
/// and nothing is asked.
#[test]
fn only_version_number_is_taken_without_values() {
    let environment = PluginEnvironment::without_server();
    let jar = luckperms_jar();
    let mut versions = saved_version_list("luckperms");
    versions
        .as_array_mut()
        .unwrap()
        .retain(|version| version["version_number"] == "v5.5.71-bukkit");
    environment
        .modrinth
        .serve_files(&mut versions, &[ServedFile::new(LUCKPERMS_FILE_NAME, &jar)]);
    environment.modrinth.mount_project("luckperms");
    environment
        .modrinth
        .mount_version_list_json("luckperms", &versions);

    let output = environment.fetch("modrinth:luckperms");

    assert_eq!(last_record(&output.stdout)[2], LUCKPERMS_SOURCE);
}

/// plg-10, plg-12: attributes whose candidates have one kind of value are skipped, and left out
/// of the source.
#[test]
fn attributes_with_one_kind_of_value_are_skipped() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert_eq!(last_record(&output.stdout)[2], LUCKPERMS_SOURCE);
}

/// plg-10: an attribute with two kinds of values takes the next value given.
#[test]
fn next_value_narrows_an_attribute_with_two_kinds() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1:paper",
        StdinConnection::NeverClosing,
        &[],
    );

    assert_eq!(fetched_file_name(&output), FAWE_PAPER_2_16_1);
}

/// plg-10: the two files of 2.14.1 have the same list of software, which counts as one kind, so
/// the next value given is the file name.
#[test]
fn equal_lists_are_one_kind_of_value() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.14.1:FastAsyncWorldEdit-Paper-2.14.1.jar",
        StdinConnection::NeverClosing,
        &[],
    );

    assert_eq!(
        fetched_file_name(&output),
        "FastAsyncWorldEdit-Paper-2.14.1.jar"
    );
}

/// plg-10: a value that no candidate has is an error showing the attribute and the values.
#[test]
fn value_that_no_candidate_has_is_an_error_with_the_values() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1:fabric",
        StdinConnection::NeverClosing,
        &[],
    );

    assert!(has_line(
        &output.stdout,
        "no file of modrinth:fastasyncworldedit:2.16.1 has the software \"fabric\"; its files \
         have these values of software: paper, spigot"
    ));
}

/// plg-10: a value left after the candidates are one file is an error.
#[test]
fn value_left_over_is_an_error() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.fetch("modrinth:luckperms:v5.5.71-bukkit:paper");

    assert!(has_line(
        &output.stdout,
        "modrinth:luckperms:v5.5.71-bukkit names one file, but these values are left over: paper"
    ));
}

/// plg-11: on a terminal, mism asks for the attribute with the candidates' values.
#[test]
fn terminal_is_asked_for_the_attribute() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1",
        StdinConnection::Terminal(b"paper\n".to_vec()),
        &[],
    );

    assert!(output.stdout.starts_with(
        "mism: modrinth:fastasyncworldedit:2.16.1 has files with these values of software:\n\
         mism: paper\nmism: spigot\nmism: software: "
    ));
}

/// plg-11: the version numbers are shown newest first.
#[test]
fn version_numbers_are_shown_newest_first() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit",
        StdinConnection::Terminal(b"2.16.1\npaper\n".to_vec()),
        &[],
    );

    assert!(output.stdout.starts_with(
        "mism: modrinth:fastasyncworldedit has files with these values of version:\n\
         mism: 2.16.1\nmism: 2.16.0\nmism: 2.15.4\n"
    ));
}

/// plg-11, plg-12, d-prompt-2: the answers typed ahead are read one per question, and choose the
/// file whose source has them.
#[test]
fn answers_choose_the_file() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit",
        StdinConnection::Terminal(b"2.16.1\npaper\n".to_vec()),
        &[],
    );

    assert_eq!(
        last_record(&output.stdout)[2],
        "modrinth:fastasyncworldedit:2.16.1:paper"
    );
}

/// plg-11: an answer that is not one of the values is an error.
#[test]
fn answer_that_is_not_a_value_is_an_error() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1",
        StdinConnection::Terminal(b"fabric\n".to_vec()),
        &[],
    );

    assert!(
        output
            .stdout
            .contains("mism: \"fabric\" is not one of these values of software: paper, spigot\n")
    );
}

/// plg-11: an empty answer is an error.
#[test]
fn empty_answer_is_an_error() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1",
        StdinConnection::Terminal(b"\n".to_vec()),
        &[],
    );

    assert!(
        output
            .stdout
            .contains("mism: \"\" is not one of these values of software: paper, spigot\n")
    );
}

/// plg-11: stdin that is not a terminal is not asked.
#[test]
fn stdin_that_is_not_a_terminal_is_not_asked() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1",
        StdinConnection::ClosedAfter(b"paper\n".to_vec()),
        &[],
    );

    assert!(!output.stdout.contains("mism: software: "));
}

/// plg-11: without asking, the error shows the attribute and the candidates' values.
#[test]
fn stdin_that_is_not_a_terminal_gets_an_error_with_the_values() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1",
        StdinConnection::ClosedAfter(b"paper\n".to_vec()),
        &[],
    );

    assert!(has_line(
        &output.stdout,
        "modrinth:fastasyncworldedit:2.16.1 has files with these values of software: paper, \
         spigot"
    ));
}

/// plg-11: with `--yes`, a terminal is not asked.
#[test]
fn yes_keeps_a_terminal_from_being_asked() {
    let output = fetch_fastasyncworldedit(
        "modrinth:fastasyncworldedit:2.16.1",
        StdinConnection::Terminal(b"paper\n".to_vec()),
        &["--yes"],
    );

    assert!(has_line(
        &output.stdout,
        "modrinth:fastasyncworldedit:2.16.1 has files with these values of software: paper, \
         spigot"
    ));
}

/// plg-12: the source mism printed chooses the same file without asking.
#[test]
fn printed_source_chooses_the_same_file_without_asking() {
    let environment = serving_fastasyncworldedit(PluginEnvironment::without_server());
    let asked = environment
        .mism_plugin(["fetch", "modrinth:fastasyncworldedit"])
        .stdin(StdinConnection::Terminal(b"2.16.1\npaper\n".to_vec()))
        .run();
    let printed_source = last_record(&asked.stdout)[2].to_owned();

    let output = environment
        .mism_plugin(["fetch", &printed_source])
        .stdin(StdinConnection::NeverClosing)
        .run();

    assert_eq!(fetched_file_name(&output), FAWE_PAPER_2_16_1);
}

/// d-pcmd-1: a subcommand interrupted while it asks leaves `plugins/` and `mism.toml` as they
/// were.
#[test]
fn interrupted_subcommand_leaves_the_server_as_it_was() {
    let environment = serving_fastasyncworldedit(PluginEnvironment::with_server("paper", "26.3"));
    environment.put_in_plugins("Other.jar", &example_jar());
    let before = environment.server_state();
    let mut mism = environment
        .mism_plugin(["inspect", "modrinth:fastasyncworldedit:2.16.1"])
        .stdin(StdinConnection::Terminal(Vec::new()))
        .spawn();
    mism.wait_for_stdout("mism: software: ");

    mism.send(Signal::SIGINT);
    mism.wait();

    assert_eq!(environment.server_state(), before);
}

/// d-prompt-1: SIGINT while mism waits for an answer ends it with status 130 at once.
#[test]
fn interruption_while_asking_exits_with_130() {
    let environment = serving_fastasyncworldedit(PluginEnvironment::without_server());
    let mut mism = environment
        .mism_plugin(["fetch", "modrinth:fastasyncworldedit"])
        .stdin(StdinConnection::Terminal(Vec::new()))
        .spawn();
    mism.wait_for_stdout("mism: version: ");

    mism.send(Signal::SIGINT);

    assert_eq!(mism.wait().exit_code, 130);
}

/// 「標準入力からの回答の読み取り」 方法 5: after an answer read from a pipe, the next output
/// starts on a new line.
#[test]
fn output_after_an_answer_from_a_pipe_starts_on_a_new_line() {
    let environment = PluginEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar("/downloads/Example.jar", &example_jar());

    let output = environment
        .mism_plugin(["fetch", &url])
        .stdin(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()))
        .run();

    assert!(
        output
            .stdout
            .contains("mism: plugin name (empty for Example): \nmism: Renamed\t"),
        "stdout: {}",
        output.stdout
    );
}

/// 「標準入力からの回答の読み取り」 方法 5: when stdin and stdout are both terminals, mism writes
/// no newline after the answer.
#[test]
fn output_after_an_answer_on_terminals_follows_on_the_same_line() {
    let environment = PluginEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar("/downloads/Example.jar", &example_jar());
    let terminal = pseudo_terminal::open();

    let mism = environment
        .mism_plugin(["fetch", &url])
        .stdin(StdinConnection::Terminal(b"Renamed\n".to_vec()))
        .stdout(terminal.slave)
        .spawn();
    let stdout = pseudo_terminal::read_until_closed(terminal.master);
    mism.wait();

    assert!(
        stdout.contains(
            "\x1b[34mmism:\x1b[0m plugin name (empty for Example): \x1b[34mmism:\x1b[0m Renamed\t"
        ),
        "stdout: {stdout}"
    );
}

/// d-modrinth-1: after SIGINT during a request, mism sends no further request.
#[test]
fn no_request_follows_an_interrupted_request() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.delay_responses(Duration::from_secs(2));
    environment.modrinth.serve_luckperms(&luckperms_jar());
    let mism = environment.mism_plugin(["fetch", LUCKPERMS_SOURCE]).spawn();
    environment.modrinth.wait_for_requests(1);

    mism.send(Signal::SIGINT);
    mism.wait();

    assert_eq!(environment.modrinth.request_count(), 1);
}

/// d-modrinth-4: HTTP status 429 hints at waiting a minute.
#[test]
fn too_many_requests_hints_at_waiting_a_minute() {
    let environment = PluginEnvironment::without_server();
    environment
        .modrinth
        .mount_status("/v2/project/luckperms", 429, "{}");

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert!(has_line(
        &output.stdout,
        "hint: wait a minute and run the command again"
    ));
}

/// d-source-1: a local file whose name does not end with `.jar` is an error.
#[test]
fn local_file_not_named_jar_is_an_error() {
    let environment = PluginEnvironment::without_server();
    let file = environment.write_local_jar("Example.zip", &example_jar());

    let output = environment.fetch(file.to_str().unwrap());

    assert!(has_line(
        &output.stdout,
        &format!(
            "{} is not a source: its name does not end with `.jar`",
            file.display()
        )
    ));
}

/// d-source-1: such a file is not stored.
#[test]
fn local_file_not_named_jar_is_not_stored() {
    let environment = PluginEnvironment::without_server();
    let file = environment.write_local_jar("Example.zip", &example_jar());

    environment.fetch(file.to_str().unwrap());

    assert_eq!(
        environment.files_in_cache(),
        Vec::<std::path::PathBuf>::new()
    );
}

/// d-source-2
#[test]
fn plugin_name_from_a_url_keeps_percent_encoding_without_query_and_fragment() {
    let environment = PluginEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar("/dl/My%20Plugin.jar", &example_jar());

    let output = environment.fetch(&format!("{url}?token=1#top"));

    assert!(has_line(
        &output.stdout,
        "plugin name from the URL: My%20Plugin"
    ));
}

/// d-source-3: `modrinth:../search` sends no request.
#[test]
fn identifier_with_a_path_sends_no_request() {
    let environment = PluginEnvironment::without_server();

    environment.fetch("modrinth:../search");

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// d-source-3: `modrinth:../search` is an error.
#[test]
fn identifier_with_a_path_is_an_error() {
    let environment = PluginEnvironment::without_server();

    let output = environment.fetch("modrinth:../search");

    assert!(
        output
            .stdout
            .starts_with("mism: \"../search\" is not a Modrinth identifier")
    );
}

/// The saved project of `slug`, whose `title` is replaced by `title`.
fn project_titled(slug: &str, title: &str) -> Value {
    let mut project = saved_project(slug);
    project["title"] = title.into();
    project
}

/// plg-6: a plugin name from Modrinth is checked too.
#[test]
fn dot_dot_title_is_an_error() {
    let environment = PluginEnvironment::without_server();
    environment
        .modrinth
        .mount_project_json("luckperms", &project_titled("luckperms", ".."));
    environment.modrinth.mount_version_list(
        "luckperms",
        &[ServedFile::new(LUCKPERMS_FILE_NAME, &luckperms_jar())],
    );

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert!(has_line(
        &output.stdout,
        "the plugin name \"..\" cannot be used: it is empty, `.`, `..`, or contains NUL"
    ));
}
