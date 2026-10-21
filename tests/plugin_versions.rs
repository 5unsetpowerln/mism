mod common;

use serde_json::{Value, json};

use common::fake_modrinth::{project_id, saved_project, saved_version_list};
use common::mism_process::Output;
use common::plugin_output::has_line;
use common::plugin_server_directory::PluginEnvironment;

const LUCKPERMS: &str = "modrinth:luckperms";
const BUKKIT_RECORD: &str = "v5.5.71-bukkit\trelease\tbukkit,folia,paper,spigot\t26.3\t-";
/// The names of the values, which come before the versions (vers-6).
const NAMES_OF_THE_VALUES: &str =
    "version\trelease_type\tsoftware\tnewest_minecraft_version\trequired_dependencies";

/// The index of LuckPerms v5.5.71-bukkit in its saved version list.
fn bukkit_index(versions: &Value) -> usize {
    versions
        .as_array()
        .unwrap()
        .iter()
        .position(|version| version["version_number"] == "v5.5.71-bukkit")
        .unwrap()
}

/// `mism plugin versions modrinth:luckperms` in `environment`, whose fake Modrinth serves the
/// saved LuckPerms project and its saved version list changed by `change`.
fn list_luckperms_in(environment: &PluginEnvironment, change: impl FnOnce(&mut Value)) -> Output {
    environment.modrinth.mount_project("luckperms");
    let mut versions = saved_version_list("luckperms");
    change(&mut versions);
    environment
        .modrinth
        .mount_version_list_json("luckperms", &versions);
    environment.mism_plugin(["versions", LUCKPERMS]).run()
}

/// `mism plugin versions modrinth:luckperms` without a target server.
fn list_luckperms(change: impl FnOnce(&mut Value)) -> Output {
    list_luckperms_in(&PluginEnvironment::without_server(), change)
}

/// `mism plugin versions modrinth:luckperms` on a server of `software` for `minecraft_version`.
fn list_luckperms_on(
    software: &str,
    minecraft_version: &str,
    change: impl FnOnce(&mut Value),
) -> Output {
    list_luckperms_in(
        &PluginEnvironment::with_server(software, minecraft_version),
        change,
    )
}

/// LuckPerms v5.5.71-bukkit with these Minecraft versions.
fn list_luckperms_bukkit_for(minecraft_versions: Value) -> Output {
    list_luckperms(|versions| {
        let index = bukkit_index(versions);
        versions[index]["game_versions"] = minecraft_versions;
    })
}

/// The line mism wrote about the version `version_number`, without `mism: `.
fn line_of<'a>(stdout: &'a str, version_number: &str) -> Option<&'a str> {
    let start = format!("mism: {version_number}\t");
    stdout
        .lines()
        .find(|line| line.starts_with(&start))
        .map(|line| &line["mism: ".len()..])
}

/// The first line mism wrote, without `mism: `.
fn first_line(stdout: &str) -> Option<&str> {
    stdout
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("mism: "))
}

/// The version number at the start of each line after the names of the values.
fn version_numbers(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .skip(1)
        .map(|line| line["mism: ".len()..].split('\t').next().unwrap())
        .collect()
}

fn saved_version_numbers(slug: &str) -> Vec<String> {
    saved_version_list(slug)
        .as_array()
        .unwrap()
        .iter()
        .map(|version| version["version_number"].as_str().unwrap().to_owned())
        .collect()
}

/// `mism plugin versions modrinth:multiverse-portals`, whose versions require Multiverse-Core,
/// without a target server.
fn list_multiverse_portals(serve_projects: impl FnOnce(&PluginEnvironment)) -> Output {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.mount_project("multiverse-portals");
    environment
        .modrinth
        .mount_version_list("multiverse-portals", &[]);
    serve_projects(&environment);
    environment
        .mism_plugin(["versions", "modrinth:multiverse-portals"])
        .run()
}

/// `mism plugin versions <argument>` without a fake response mounted.
fn list_invalid(argument: &str) -> (PluginEnvironment, Output) {
    let environment = PluginEnvironment::without_server();
    let output = environment.mism_plugin(["versions", argument]).run();
    (environment, output)
}

/// vers-1: the versions are written newest first, also when Modrinth returns them oldest first.
#[test]
fn versions_are_written_newest_first() {
    let output = list_luckperms(|versions| {
        versions.as_array_mut().unwrap().reverse();
    });

    assert_eq!(
        version_numbers(&output.stdout),
        saved_version_numbers("luckperms")
    );
}

/// vers-1 (「mism plugin versions の手順」 方法 6): versions published at the same time keep the
/// order Modrinth returned them in.
#[test]
fn equally_new_versions_keep_modrinths_order() {
    let output = list_luckperms(|versions| {
        versions[0]["date_published"] = versions[1]["date_published"].clone();
    });

    assert_eq!(
        version_numbers(&output.stdout)[..2],
        ["v5.5.87-neoforge", "v5.5.85-neoforge"]
    );
}

/// vers-1: the listing exits with status 0.
#[test]
fn listing_exits_with_status_0() {
    let output = list_luckperms(|_| {});

    assert_eq!(output.exit_code, 0);
}

/// vers-2: the record of a version without a target server.
#[test]
fn record_of_a_version() {
    let output = list_luckperms(|_| {});

    assert_eq!(
        line_of(&output.stdout, "v5.5.71-bukkit"),
        Some(BUKKIT_RECORD)
    );
}

/// vers-2: the required dependencies are their identifiers.
#[test]
fn required_dependencies_are_their_identifiers() {
    let output = list_multiverse_portals(|environment| {
        environment.modrinth.mount_projects_multiverse_core();
    });

    assert_eq!(
        line_of(&output.stdout, "5.3.0"),
        Some("5.3.0\trelease\tbukkit,paper,purpur,spigot\t26.3\tmultiverse-core")
    );
}

/// vers-2: the release type is written as Modrinth states it.
#[test]
fn release_type_is_written() {
    let output = list_multiverse_portals(|environment| {
        environment.modrinth.mount_projects_multiverse_core();
    });

    assert_eq!(
        line_of(&output.stdout, "5.3.0-pre").map(|line| line.split('\t').nth(1)),
        Some(Some("beta"))
    );
}

/// d-compat-5
#[test]
fn newest_minecraft_version_includes_snapshots() {
    let output = list_luckperms_bukkit_for(json!(["26.3", "26.4-snapshot-5", "25w46a"]));

    assert_eq!(
        line_of(&output.stdout, "v5.5.71-bukkit"),
        Some("v5.5.71-bukkit\trelease\tbukkit,folia,paper,spigot\t26.4-snapshot-5\t-")
    );
}

/// d-compat-6
#[test]
fn newest_minecraft_version_of_unparsable_versions_is_a_hyphen() {
    let output = list_luckperms_bukkit_for(json!(["25w46a", "26w03b"]));

    assert_eq!(
        line_of(&output.stdout, "v5.5.71-bukkit"),
        Some("v5.5.71-bukkit\trelease\tbukkit,folia,paper,spigot\t-\t-")
    );
}

/// vers-3: the example of the specification, on a Paper server for 26.3.
#[test]
fn version_for_the_server_matches() {
    let output = list_luckperms_on("paper", "26.3", |_| {});

    assert_eq!(
        line_of(&output.stdout, "v5.5.71-bukkit"),
        Some(format!("{BUKKIT_RECORD}\tmatch").as_str())
    );
}

/// vers-3: the server software of a Folia server is `folia`.
#[test]
fn version_for_folia_matches_a_folia_server() {
    let output = list_luckperms_on("folia", "26.3", |_| {});

    assert_eq!(
        line_of(&output.stdout, "v5.5.71-bukkit"),
        Some(format!("{BUKKIT_RECORD}\tmatch").as_str())
    );
}

/// vers-3: a version for the software but not the Minecraft version is `software-only`.
#[test]
fn version_for_another_minecraft_version_is_software_only() {
    let output = list_luckperms_on("paper", "1.7.10", |_| {});

    assert_eq!(
        line_of(&output.stdout, "v5.5.71-bukkit"),
        Some("v5.5.71-bukkit\trelease\tbukkit,folia,paper,spigot\t26.3\t-\tsoftware-only")
    );
}

/// vers-3: a version for other server software is `mismatch`.
#[test]
fn version_for_other_software_is_a_mismatch() {
    let output = list_luckperms_on("paper", "26.3", |_| {});

    assert_eq!(
        line_of(&output.stdout, "v5.5.87-neoforge"),
        Some("v5.5.87-neoforge\trelease\tneoforge\t26.3\t-\tmismatch")
    );
}

/// vers-3: a version without server software is `unknown`.
#[test]
fn version_without_software_is_unknown() {
    let output = list_luckperms_on("paper", "26.3", |versions| {
        let index = bukkit_index(versions);
        versions[index]["loaders"] = json!([]);
    });

    assert_eq!(
        line_of(&output.stdout, "v5.5.71-bukkit"),
        Some("v5.5.71-bukkit\trelease\t-\t26.3\t-\tunknown")
    );
}

/// plg-2: without `mism.toml`, `mism plugin versions` goes on without a target server.
#[test]
fn versions_without_mism_toml_succeeds() {
    let output = list_luckperms(|_| {});

    assert_eq!(output.exit_code, 0);
}

/// vers-4: an argument without `modrinth:` is an error.
#[test]
fn argument_without_modrinth_is_an_error() {
    let (_environment, output) = list_invalid("luckperms");

    assert!(has_line(
        &output.stdout,
        "\"luckperms\" is not `modrinth:` followed by an identifier and nothing else"
    ));
}

/// vers-4: a URL is not an argument of `mism plugin versions`.
#[test]
fn url_argument_is_an_error() {
    let (_environment, output) = list_invalid("https://example.com/LuckPerms.jar");

    assert!(has_line(
        &output.stdout,
        "\"https://example.com/LuckPerms.jar\" is not `modrinth:` followed by an identifier and \
         nothing else"
    ));
}

/// vers-4: an argument with a value after the identifier is an error.
#[test]
fn argument_with_a_value_is_an_error() {
    let (_environment, output) = list_invalid("modrinth:luckperms:v5.5.71-bukkit");

    assert!(has_line(
        &output.stdout,
        "\"modrinth:luckperms:v5.5.71-bukkit\" is not `modrinth:` followed by an identifier and \
         nothing else"
    ));
}

/// vers-4: an identifier of characters Modrinth does not use is an error.
#[test]
fn argument_with_an_invalid_identifier_is_an_error() {
    let (_environment, output) = list_invalid("modrinth:../search");

    assert!(has_line(
        &output.stdout,
        "\"../search\" is not a Modrinth identifier: it must be 3 to 64 characters, each an ASCII \
         letter or digit or one of _!@$()`.+,\"-'"
    ));
}

/// vers-4: an invalid argument exits with status 1 (err-9).
#[test]
fn invalid_argument_exits_with_status_1() {
    let (_environment, output) = list_invalid("luckperms");

    assert_eq!(output.exit_code, 1);
}

/// vers-4 (「mism plugin versions の手順」 方法 1): an invalid argument sends no request.
#[test]
fn invalid_argument_sends_no_request() {
    let (environment, _output) = list_invalid("modrinth:luckperms:v5.5.71-bukkit");

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// plg-3: an identifier that differs from the slug in case is an error.
#[test]
fn identifier_differing_from_the_slug_in_case_is_an_error() {
    let environment = PluginEnvironment::without_server();
    environment
        .modrinth
        .mount_project_json("LuckPerms", &saved_project("luckperms"));

    let output = environment
        .mism_plugin(["versions", "modrinth:LuckPerms"])
        .run();

    assert!(has_line(
        &output.stdout,
        "\"LuckPerms\" is not the identifier of the Modrinth project it names, whose identifier \
         is \"luckperms\""
    ));
}

/// `mism plugin versions modrinth:luckperms` where Modrinth answers the version list with
/// status 500.
fn list_failing_at_the_version_list() -> Output {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.mount_project("luckperms");
    environment.modrinth.mount_status(
        &format!("/v2/project/{}/version", project_id("luckperms")),
        500,
        "{}",
    );
    environment.mism_plugin(["versions", LUCKPERMS]).run()
}

/// vers-5: a failed request exits with status 1.
#[test]
fn failed_request_exits_with_status_1() {
    assert_eq!(list_failing_at_the_version_list().exit_code, 1);
}

/// vers-5: a failed request is reported as err-9 says, with the hint at the network.
#[test]
fn failed_request_hints_at_the_network() {
    assert!(has_line(
        &list_failing_at_the_version_list().stdout,
        "hint: check your network connection"
    ));
}

/// vers-5: a failed request writes no names of the values.
#[test]
fn failed_request_writes_no_names_of_the_values() {
    assert!(!has_line(
        &list_failing_at_the_version_list().stdout,
        NAMES_OF_THE_VALUES
    ));
}

/// vers-5 (「mism plugin versions の手順」 方法 7): a request that fails after the version list
/// was received writes no version.
#[test]
fn failed_request_for_the_dependencies_writes_no_version() {
    let output = list_multiverse_portals(|environment| {
        environment.modrinth.mount_status("/v2/projects", 500, "{}");
    });

    assert!(!output.stdout.contains('\t'), "stdout: {}", output.stdout);
}

/// vers-6: without a target server, the names of the values come first.
#[test]
fn names_of_the_values_come_first() {
    let output = list_luckperms(|_| {});

    assert_eq!(first_line(&output.stdout), Some(NAMES_OF_THE_VALUES));
}

/// vers-6: with a target server, the names of the values end with `declared`.
#[test]
fn names_of_the_values_end_with_declared_on_a_server() {
    let output = list_luckperms_on("paper", "26.3", |_| {});

    assert_eq!(
        first_line(&output.stdout),
        Some(format!("{NAMES_OF_THE_VALUES}\tdeclared").as_str())
    );
}

/// vers-6: for a plugin without versions, mism writes the names of the values and nothing else.
#[test]
fn names_of_the_values_are_written_for_a_plugin_without_versions() {
    let output = list_luckperms(|versions| *versions = json!([]));

    assert_eq!(output.stdout, format!("mism: {NAMES_OF_THE_VALUES}\n"));
}
