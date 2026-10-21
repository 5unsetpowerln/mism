mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use common::fake_modrinth::{LUCKPERMS_FILE_NAME, LUCKPERMS_SOURCE, sha256_hex};
use common::mism_process::Output;
use common::plugin_output::{has_line, last_record, line_index};
use common::plugin_server_directory::{PluginEnvironment, Record, ServerState, mism_toml_text};
use common::test_plugin_jar::{jar_with_plugin_yml, jar_without_declaration, plugin_yml};

const TAKES_EFFECT_NOTICE: &str = "the change takes effect when the server starts next";

fn example_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("Example", "1.0", "1.21"))
}

fn luckperms_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("LuckPerms", "5.5.71", "1.13"))
}

/// A jar of another plugin, which conflicts with neither `Example` nor `LuckPerms`.
fn other_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("Other", "1.0", "1.13"))
}

/// A Paper 26.3 server directory, with LuckPerms v5.5.71-bukkit fetched from the fake Modrinth.
fn luckperms_fetched() -> PluginEnvironment {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.modrinth.serve_luckperms(&luckperms_jar());
    environment.fetch(LUCKPERMS_SOURCE);
    environment
}

fn place_luckperms(environment: &PluginEnvironment) -> Output {
    environment.mism_plugin(["place", LUCKPERMS_SOURCE]).run()
}

/// `environment`'s local `Example.jar` with `bytes`, fetched; returns its path, which is its
/// source.
fn fetch_local_example(environment: &PluginEnvironment, bytes: &[u8]) -> PathBuf {
    let jar = environment.write_local_jar("Example.jar", bytes);
    environment.fetch(jar.to_str().unwrap());
    jar
}

/// Fetches the local `Example.jar` with `bytes` on a server of `software` for 26.3, prepared by
/// `prepare`, and places it. Returns the server's state before the placement too.
fn place_example_on(
    software: &str,
    bytes: &[u8],
    prepare: impl FnOnce(&PluginEnvironment),
) -> (PluginEnvironment, Output, ServerState) {
    let environment = PluginEnvironment::with_server(software, "26.3");
    prepare(&environment);
    let jar = fetch_local_example(&environment, bytes);
    let before = environment.server_state();
    let output = environment
        .mism_plugin(["place".as_ref(), jar.as_os_str()])
        .run();
    (environment, output, before)
}

/// place-1: a source that was not fetched is an error.
#[test]
fn source_not_fetched_is_an_error() {
    let environment = PluginEnvironment::with_server("paper", "26.3");

    let output = place_luckperms(&environment);

    assert!(has_line(
        &output.stdout,
        &format!("the cache directory has no jar fetched from {LUCKPERMS_SOURCE}")
    ));
}

/// place-1: the error tells to fetch the plugin and give the printed source.
#[test]
fn source_not_fetched_hints_at_fetching() {
    let environment = PluginEnvironment::with_server("paper", "26.3");

    let output = place_luckperms(&environment);

    assert!(has_line(
        &output.stdout,
        &format!("hint: run `mism plugin fetch {LUCKPERMS_SOURCE}` and give the source it prints")
    ));
}

/// place-1: a Modrinth source that is not the one the jar was fetched with does not find it.
#[test]
fn other_modrinth_source_does_not_find_the_jar() {
    let environment = luckperms_fetched();

    let output = environment
        .mism_plugin(["place", "modrinth:luckperms"])
        .run();

    assert!(has_line(
        &output.stdout,
        "the cache directory has no jar fetched from modrinth:luckperms"
    ));
}

/// place-1: a local jar file finds the cached jar with its SHA-256, however it was fetched.
#[test]
fn local_jar_finds_the_cached_jar_with_its_sha256() {
    let environment = luckperms_fetched();
    let jar = environment.write_local_jar(LUCKPERMS_FILE_NAME, &luckperms_jar());

    let output = environment
        .mism_plugin(["place".as_ref(), jar.as_os_str()])
        .run();

    assert_eq!(output.exit_code, 0);
}

/// place-1: placing sends no request.
#[test]
fn place_sends_no_request() {
    let environment = luckperms_fetched();
    let requests_before = environment.modrinth.request_count();

    place_luckperms(&environment);

    assert_eq!(environment.modrinth.request_count(), requests_before);
}

/// place-2: the jar is directly in `plugins/`, which did not exist.
#[test]
fn jar_is_placed_in_plugins() {
    let environment = luckperms_fetched();

    place_luckperms(&environment);

    assert_eq!(
        fs::read(environment.plugins().join(LUCKPERMS_FILE_NAME)).unwrap(),
        luckperms_jar()
    );
}

/// place-2, plg-4, plg-12: `mism.toml` records the plugin name, the source, the jar file name,
/// and the SHA-256.
#[test]
fn placed_plugin_is_recorded() {
    let environment = luckperms_fetched();

    place_luckperms(&environment);

    assert_eq!(
        environment.read_mism_toml(),
        mism_toml_text(
            "paper",
            "26.3",
            &[Record {
                name: "LuckPerms",
                source: LUCKPERMS_SOURCE,
                file_name: LUCKPERMS_FILE_NAME,
                sha256: &sha256_hex(&luckperms_jar()),
            }]
        )
    );
}

/// `mism.toml` with a record of `LuckPerms` whose jar is `LP.jar`.
fn record_named_luckperms(environment: &PluginEnvironment) {
    environment.write_mism_toml(&mism_toml_text(
        "paper",
        "26.3",
        &[Record {
            name: "LuckPerms",
            source: "modrinth:luckperms:v5.5.70-bukkit",
            file_name: "LP.jar",
            sha256: &sha256_hex(b"LP"),
        }],
    ));
}

/// place-3: a record with the same plugin name is an error.
#[test]
fn record_with_the_same_name_is_an_error() {
    let environment = luckperms_fetched();
    record_named_luckperms(&environment);

    let output = place_luckperms(&environment);

    assert!(has_line(
        &output.stdout,
        "mism.toml already has a record of the plugin LuckPerms"
    ));
}

/// place-3: such a placement changes nothing.
#[test]
fn record_with_the_same_name_changes_nothing() {
    let environment = luckperms_fetched();
    record_named_luckperms(&environment);
    let before = environment.server_state();

    place_luckperms(&environment);

    assert_eq!(environment.server_state(), before);
}

/// place-4: a jar with the same file name in `plugins/` is an error.
#[test]
fn jar_with_the_same_name_is_an_error() {
    let environment = luckperms_fetched();
    let existing = environment.put_in_plugins(LUCKPERMS_FILE_NAME, &other_jar());

    let output = place_luckperms(&environment);

    assert!(has_line(
        &output.stdout,
        &format!("{} already exists", existing.display())
    ));
}

/// place-4: such a placement changes nothing.
#[test]
fn jar_with_the_same_name_changes_nothing() {
    let environment = luckperms_fetched();
    environment.put_in_plugins(LUCKPERMS_FILE_NAME, &other_jar());
    let before = environment.server_state();

    place_luckperms(&environment);

    assert_eq!(environment.server_state(), before);
}

/// place-5, plg-7: the record of the placed plugin, then the notice, end the output.
#[test]
fn placement_ends_with_its_record_and_the_notice() {
    let environment = luckperms_fetched();

    let output = place_luckperms(&environment);

    assert!(output.stdout.ends_with(&format!(
        "\nmism: LuckPerms\tv5.5.71-bukkit\t{LUCKPERMS_FILE_NAME}\nmism: {TAKES_EFFECT_NOTICE}\n"
    )));
}

/// place-5: a jar not from Modrinth has `-` as its version number.
#[test]
fn local_jar_record_has_no_version_number() {
    let (_environment, output, _before) = place_example_on("paper", &example_jar(), |_| {});

    assert!(output.stdout.ends_with(&format!(
        "\nmism: Example\t-\tExample.jar\nmism: {TAKES_EFFECT_NOTICE}\n"
    )));
}

/// place-5: a successful placement exits with status 0.
#[test]
fn successful_placement_exits_with_status_0() {
    let environment = luckperms_fetched();

    let output = place_luckperms(&environment);

    assert_eq!(output.exit_code, 0);
}

/// place-6: the inspection of the jar precedes the record of the placed plugin.
#[test]
fn inspection_precedes_the_record() {
    let environment = luckperms_fetched();

    let output = place_luckperms(&environment);

    let inspected = line_index(&output.stdout, "name\tLuckPerms");
    let placed = line_index(
        &output.stdout,
        &format!("LuckPerms\tv5.5.71-bukkit\t{LUCKPERMS_FILE_NAME}"),
    );
    assert!(matches!((inspected, placed), (Some(inspected), Some(placed)) if inspected < placed));
}

/// place-6: `incompatible` stops the placement after the inspection was written.
#[test]
fn incompatible_stops_after_the_inspection() {
    let (_environment, output, _before) = place_example_on("folia", &example_jar(), |_| {});

    let judged = line_index(
        &output.stdout,
        "incompatible\tdoes not declare Folia support",
    );
    let failed = line_index(
        &output.stdout,
        "the inspection found `incompatible`, so Example.jar is not placed",
    );
    assert!(matches!((judged, failed), (Some(judged), Some(failed)) if judged < failed));
}

/// place-6: `incompatible` changes nothing.
#[test]
fn incompatible_changes_nothing() {
    let (environment, _output, before) = place_example_on("folia", &example_jar(), |_| {});

    assert_eq!(environment.server_state(), before);
}

/// Puts `Old.jar`, which declares the plugin `Example`, in `plugins/`.
fn put_conflicting_jar(environment: &PluginEnvironment) {
    environment.put_in_plugins("Old.jar", &example_jar());
}

/// place-6: `conflict` stops the placement.
#[test]
fn conflict_stops_the_placement() {
    let (_environment, output, _before) =
        place_example_on("paper", &example_jar(), put_conflicting_jar);

    assert!(has_line(
        &output.stdout,
        "the inspection found `conflict`, so Example.jar is not placed"
    ));
}

/// place-6: `conflict` changes nothing.
#[test]
fn conflict_changes_nothing() {
    let (environment, _output, before) =
        place_example_on("paper", &example_jar(), put_conflicting_jar);

    assert_eq!(environment.server_state(), before);
}

/// place-6: the error for `conflict` tells to move the conflicting jar out of `plugins/`.
#[test]
fn conflict_hints_at_moving_the_jar_out() {
    let (environment, output, _before) =
        place_example_on("paper", &example_jar(), put_conflicting_jar);

    assert!(has_line(
        &output.stdout,
        &format!(
            "hint: move {} out of plugins/",
            environment.plugins().join("Old.jar").display()
        )
    ));
}

/// place-6: `missing` alone does not stop the placement.
#[test]
fn missing_alone_does_not_stop_the_placement() {
    let bytes = jar_with_plugin_yml(&format!(
        "{}depend: [Vault]\n",
        plugin_yml("Example", "1.0", "1.21")
    ));

    let (_environment, output, _before) = place_example_on("paper", &bytes, |_| {});

    assert_eq!(output.exit_code, 0);
}

/// place-6: a jar without a declaration is the error of insp-4.
#[test]
fn jar_without_a_declaration_is_an_error() {
    let (_environment, output, _before) =
        place_example_on("paper", &jar_without_declaration(), |_| {});

    assert!(
        output
            .stdout
            .ends_with(" has neither plugin.yml nor paper-plugin.yml, so it declares no plugin\n")
    );
}

/// place-6: a jar without a declaration changes nothing.
#[test]
fn jar_without_a_declaration_changes_nothing() {
    let (environment, _output, before) =
        place_example_on("paper", &jar_without_declaration(), |_| {});

    assert_eq!(environment.server_state(), before);
}

/// Another jar of the plugin `Example`, which replaces the cached one after the fetch.
fn changed_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("Example", "9.9", "1.21"))
}

/// Fetches the local `Example.jar`, then replaces the cached jar with `changed_jar`.
fn change_the_cached_jar(environment: &PluginEnvironment) -> PathBuf {
    let jar = environment.write_local_jar("Example.jar", &example_jar());
    let fetched = environment.fetch(jar.to_str().unwrap());
    let cached = PathBuf::from(last_record(&fetched.stdout)[4]);
    fs::write(&cached, changed_jar()).unwrap();
    cached
}

/// d-pdir-1: a cached jar whose SHA-256 is not its directory's name is an error.
#[test]
fn changed_cached_jar_is_an_error() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let cached = change_the_cached_jar(&environment);

    // The local jar has the original content, so it finds the changed cached jar by its
    // directory.
    let output = environment
        .mism_plugin(["place", "../outside/Example.jar"])
        .run();

    assert!(output.stdout.contains(&format!(
        "mism: the SHA-256 of {} is {}, not the name of its directory in the cache directory\n",
        cached.display(),
        sha256_hex(&changed_jar())
    )));
}

/// d-pdir-1: a changed cached jar changes nothing.
#[test]
fn changed_cached_jar_changes_nothing() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    change_the_cached_jar(&environment);
    let before = environment.server_state();

    environment
        .mism_plugin(["place", "../outside/Example.jar"])
        .run();

    assert_eq!(environment.server_state(), before);
}

/// d-pcache-2: after two different jars were fetched from the same URL, `place` with that URL
/// reports the paths of both.
#[test]
fn two_jars_from_one_url_are_an_error_naming_both() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let url = environment
        .modrinth
        .mount_jar_once("/downloads/Example.jar", &example_jar());
    environment.modrinth.mount_jar(
        "/downloads/Example.jar",
        &jar_with_plugin_yml(&plugin_yml("Example", "2.0", "1.21")),
    );
    let first = PathBuf::from(last_record(&environment.fetch(&url).stdout)[4]);
    let second = PathBuf::from(last_record(&environment.fetch(&url).stdout)[4]);

    let output = environment.mism_plugin(["place", &url]).run();

    assert!(
        output.stdout.contains(&first.display().to_string())
            && output.stdout.contains(&second.display().to_string())
    );
}

/// plg-8: when writing `mism.toml` fails after the jar was placed, `plugins/` and `mism.toml`
/// are as they were.
#[test]
fn failure_to_write_mism_toml_leaves_the_server_as_it_was() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let jar = fetch_local_example(&environment, &example_jar());
    fs::create_dir(environment.plugins()).unwrap();
    let before = environment.server_state();
    // The jar can be placed in plugins/, but mism.toml cannot be replaced in the read-only
    // server directory.
    fs::set_permissions(environment.server(), fs::Permissions::from_mode(0o555)).unwrap();

    environment
        .mism_plugin(["place".as_ref(), jar.as_os_str()])
        .run();

    fs::set_permissions(environment.server(), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(environment.server_state(), before);
}
