mod common;

use serde_json::{Value, json};

use common::datapack_server_directory::DatapackEnvironment;
use common::fake_modrinth::{project_id, saved_project, saved_version_list};
use common::mism_process::Output;
use common::plugin_output::{has_line, reported_an_error};

const TRUE_ENDING: &str = "modrinth:true-ending";
const STELLARITY_PATCH: &str = "stellarity_x_true_ending_patch";
const TRUE_ENDING_1_1_4D_RECORD: &str = "1.1.4d\trelease\t26.1.2\t-";
/// The names of the values, which come before the versions (dvers-6).
const NAMES_OF_THE_VALUES: &str =
    "version\trelease_type\tnewest_minecraft_version\trequired_dependencies";

/// `mism datapack versions modrinth:true-ending` in `environment`, whose fake Modrinth serves the
/// saved True Ending project and its saved version list changed by `change`.
fn list_true_ending_in(
    environment: &DatapackEnvironment,
    change: impl FnOnce(&mut Value),
) -> Output {
    environment.modrinth.mount_project("true-ending");
    let mut versions = saved_version_list("true-ending");
    change(&mut versions);
    environment
        .modrinth
        .mount_version_list_json("true-ending", &versions);
    environment.mism_datapack(["versions", TRUE_ENDING]).run()
}

/// `mism datapack versions modrinth:true-ending` without a target server.
fn list_true_ending(change: impl FnOnce(&mut Value)) -> Output {
    list_true_ending_in(&DatapackEnvironment::without_server(), change)
}

/// `mism datapack versions modrinth:true-ending` on a server of `software` for
/// `minecraft_version`.
fn list_true_ending_on(software: &str, minecraft_version: &str) -> Output {
    list_true_ending_in(
        &DatapackEnvironment::with_server(software, minecraft_version),
        |_| {},
    )
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

fn is_for_datapacks(version: &Value) -> bool {
    version["loaders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|loader| loader == "datapack")
}

/// The version numbers of the saved datapack versions of `slug`, in the saved order, which is
/// newest first.
fn saved_datapack_version_numbers(slug: &str) -> Vec<String> {
    saved_version_list(slug)
        .as_array()
        .unwrap()
        .iter()
        .filter(|version| is_for_datapacks(version))
        .map(|version| version["version_number"].as_str().unwrap().to_owned())
        .collect()
}

/// `mism datapack versions modrinth:stellarity_x_true_ending_patch`, whose datapack versions
/// require True Ending and Stellarity, without a target server.
fn list_stellarity_patch(serve_projects: impl FnOnce(&DatapackEnvironment)) -> Output {
    let environment = DatapackEnvironment::without_server();
    environment.modrinth.mount_project(STELLARITY_PATCH);
    environment
        .modrinth
        .mount_version_list(STELLARITY_PATCH, &[]);
    serve_projects(&environment);
    environment
        .mism_datapack(["versions", &format!("modrinth:{STELLARITY_PATCH}")])
        .run()
}

/// `mism datapack versions <argument>` without a fake response mounted.
fn list_invalid(argument: &str) -> (DatapackEnvironment, Output) {
    let environment = DatapackEnvironment::without_server();
    let output = environment.mism_datapack(["versions", argument]).run();
    (environment, output)
}

/// dvers-1: the datapack versions, and only they, are written newest first, also when Modrinth
/// returns them oldest first.
#[test]
fn datapack_versions_are_written_newest_first() {
    let output = list_true_ending(|versions| {
        versions.as_array_mut().unwrap().reverse();
    });

    assert_eq!(
        version_numbers(&output.stdout),
        saved_datapack_version_numbers("true-ending")
    );
}

/// dvers-1 (「mism plugin versions と mism datapack versions の手順」 方法 6): versions published
/// at the same time keep the order Modrinth returned them in.
#[test]
fn equally_new_versions_keep_modrinths_order() {
    let output = list_true_ending(|versions| {
        let date = versions[31]["date_published"].clone();
        // 1.1.0a follows 1.1.0b in the saved list, against the order of their strings.
        versions[33]["date_published"] = date;
    });

    let tied: Vec<&str> = version_numbers(&output.stdout)
        .into_iter()
        .filter(|number| ["1.1.0b", "1.1.0a"].contains(number))
        .collect();
    assert_eq!(tied, ["1.1.0b", "1.1.0a"]);
}

/// dvers-1: the listing exits with status 0.
#[test]
fn listing_exits_with_status_0() {
    let output = list_true_ending(|_| {});

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dp-2: without `mism.toml`, `mism datapack versions` goes on without a target server, so it
/// judges no version.
#[test]
fn versions_without_mism_toml_judge_no_version() {
    let output = list_true_ending(|_| {});

    assert_eq!(
        line_of(&output.stdout, "1.1.4d"),
        Some(TRUE_ENDING_1_1_4D_RECORD)
    );
}

/// dvers-2: the record of a version without a target server.
#[test]
fn record_of_a_version() {
    let output = list_true_ending(|_| {});

    assert_eq!(
        line_of(&output.stdout, "1.1.4d"),
        Some(TRUE_ENDING_1_1_4D_RECORD)
    );
}

/// dvers-2: the newest Minecraft version is the newest release, not the greatest string.
#[test]
fn newest_minecraft_version_is_the_newest_release() {
    let output = list_true_ending(|_| {});

    assert_eq!(
        line_of(&output.stdout, "1.1.4c"),
        Some("1.1.4c\trelease\t1.21.10\t-")
    );
}

/// dvers-2: the required dependencies are their identifiers, and the release type is written as
/// Modrinth states it.
#[test]
fn required_dependencies_are_their_identifiers() {
    let output = list_stellarity_patch(|environment| {
        environment
            .modrinth
            .mount_projects("stellarity-true-ending", &["MCnBYP0b", "bZgeDzN8"]);
    });

    assert_eq!(
        line_of(&output.stdout, "1.0.0"),
        Some("1.0.0\talpha\t26.1.2\tstellarity,true-ending")
    );
}

/// dvers-3: the example of the specification, on a server for 26.1.2.
#[test]
fn version_for_the_servers_minecraft_version_matches() {
    let output = list_true_ending_on("paper", "26.1.2");

    assert_eq!(
        line_of(&output.stdout, "1.1.4d"),
        Some(format!("{TRUE_ENDING_1_1_4D_RECORD}\tmatch").as_str())
    );
}

/// dvers-3: the server software does not matter for datapacks.
#[test]
fn version_for_the_minecraft_version_matches_a_folia_server() {
    let output = list_true_ending_on("folia", "26.1.2");

    assert_eq!(
        line_of(&output.stdout, "1.1.4d"),
        Some(format!("{TRUE_ENDING_1_1_4D_RECORD}\tmatch").as_str())
    );
}

/// dvers-3: a version without the server's Minecraft version is `mismatch`.
#[test]
fn version_for_other_minecraft_versions_is_a_mismatch() {
    let output = list_true_ending_on("paper", "26.1.2");

    assert_eq!(
        line_of(&output.stdout, "1.1.4c"),
        Some("1.1.4c\trelease\t1.21.10\t-\tmismatch")
    );
}

/// dvers-4: an argument without `modrinth:` is an error.
#[test]
fn argument_without_modrinth_is_an_error() {
    let (_environment, output) = list_invalid("true-ending");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dvers-4: a URL is not an argument of `mism datapack versions`.
#[test]
fn url_argument_is_an_error() {
    let (_environment, output) = list_invalid("https://example.com/true-ending.zip");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dvers-4: an argument with a value after the identifier is an error.
#[test]
fn argument_with_a_value_is_an_error() {
    let (_environment, output) = list_invalid("modrinth:true-ending:1.1.4d");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dvers-4: an identifier of characters Modrinth does not use is an error.
#[test]
fn argument_with_an_invalid_identifier_is_an_error() {
    let (_environment, output) = list_invalid("modrinth:../search");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dvers-4 (「mism plugin versions と mism datapack versions の手順」 方法 1): an invalid
/// argument sends no request.
#[test]
fn invalid_argument_sends_no_request() {
    let (environment, _output) = list_invalid("modrinth:true-ending:1.1.4d");

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// dp-4: an identifier that differs from the slug in case is an error.
#[test]
fn identifier_differing_from_the_slug_in_case_is_an_error() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .mount_project_json("True-Ending", &saved_project("true-ending"));

    let output = environment
        .mism_datapack(["versions", "modrinth:True-Ending"])
        .run();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// `mism datapack versions modrinth:true-ending` where Modrinth answers the version list with
/// status 500.
fn list_failing_at_the_version_list() -> Output {
    let environment = DatapackEnvironment::without_server();
    environment.modrinth.mount_project("true-ending");
    environment.modrinth.mount_status(
        &format!("/v2/project/{}/version", project_id("true-ending")),
        500,
        "{}",
    );
    environment.mism_datapack(["versions", TRUE_ENDING]).run()
}

/// dvers-5: a failed request is reported as err-9 says.
#[test]
fn failed_request_is_an_error() {
    let output = list_failing_at_the_version_list();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dvers-5: a failed request writes no names of the values.
#[test]
fn failed_request_writes_no_names_of_the_values() {
    assert!(!has_line(
        &list_failing_at_the_version_list().stdout,
        NAMES_OF_THE_VALUES
    ));
}

/// dvers-5 (「mism plugin versions と mism datapack versions の手順」 方法 7): a request that
/// fails after the version list was received writes no version.
#[test]
fn failed_request_for_the_dependencies_writes_no_version() {
    let output = list_stellarity_patch(|environment| {
        environment.modrinth.mount_status("/v2/projects", 500, "{}");
    });

    assert!(!output.stdout.contains('\t'), "stdout: {}", output.stdout);
}

/// dvers-6: without a target server, the names of the values come first.
#[test]
fn names_of_the_values_come_first() {
    let output = list_true_ending(|_| {});

    assert_eq!(first_line(&output.stdout), Some(NAMES_OF_THE_VALUES));
}

/// dvers-6: with a target server, the names of the values end with `declared`.
#[test]
fn names_of_the_values_end_with_declared_on_a_server() {
    let output = list_true_ending_on("paper", "26.1.2");

    assert_eq!(
        first_line(&output.stdout),
        Some(format!("{NAMES_OF_THE_VALUES}\tdeclared").as_str())
    );
}

/// dvers-6: for a project with mod versions only, mism writes the names of the values and
/// nothing else.
#[test]
fn names_of_the_values_are_written_for_a_project_without_datapack_versions() {
    let output = list_true_ending(|versions| {
        versions
            .as_array_mut()
            .unwrap()
            .retain(|version| !is_for_datapacks(version));
    });

    assert_eq!(output.stdout, format!("mism: {NAMES_OF_THE_VALUES}\n"));
}

/// dvers-6: for a project without versions, mism writes the names of the values and nothing
/// else.
#[test]
fn names_of_the_values_are_written_for_a_project_without_versions() {
    let output = list_true_ending(|versions| *versions = json!([]));

    assert_eq!(output.stdout, format!("mism: {NAMES_OF_THE_VALUES}\n"));
}
