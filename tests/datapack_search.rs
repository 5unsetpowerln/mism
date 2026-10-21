mod common;

use serde_json::{Value, json};

use common::datapack_server_directory::DatapackEnvironment;
use common::fake_modrinth::{project_id, saved_version_list};
use common::mism_process::Output;
use common::plugin_output::reported_an_error;

const TERM: &str = "true ending";
/// The identifiers the saved search for `true ending` found, in Modrinth's order.
const FOUND: [&str; 3] = [
    "true-ending",
    "stellarity_x_true_ending_patch",
    "vanilla-true-ending",
];

/// An environment whose fake Modrinth serves the saved datapack search for `true ending` and the
/// saved version list of each datapack it found, except those that `serve_instead` serves.
fn serving_the_search(serve_instead: &[&str]) -> DatapackEnvironment {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .mount_search(TERM, "datapack", "datapack-true-ending");
    for slug in FOUND
        .into_iter()
        .filter(|slug| !serve_instead.contains(slug))
    {
        environment.modrinth.mount_version_list(slug, &[]);
    }
    environment
}

fn search(environment: &DatapackEnvironment, extra: &[&str]) -> Output {
    environment
        .mism_datapack(["search", TERM])
        .arguments(extra)
        .run()
}

/// `mism datapack search true ending` with the saved responses.
fn search_with_saved_responses() -> (DatapackEnvironment, Output) {
    let environment = serving_the_search(&[]);
    let output = search(&environment, &[]);
    (environment, output)
}

/// `mism datapack search true ending` with the version list of `slug` replaced by `change`
/// applied to its saved one.
fn search_with_versions_changed(slug: &str, change: impl FnOnce(&mut Value)) -> Output {
    let environment = serving_the_search(&[slug]);
    let mut versions = saved_version_list(slug);
    change(&mut versions);
    environment
        .modrinth
        .mount_version_list_json(slug, &versions);
    search(&environment, &[])
}

/// The line mism wrote about the datapack `identifier`, without `mism: `.
fn line_of<'a>(stdout: &'a str, identifier: &str) -> Option<&'a str> {
    let start = format!("mism: modrinth:{identifier}\t");
    stdout
        .lines()
        .find(|line| line.starts_with(&start))
        .map(|line| &line["mism: ".len()..])
}

/// `mism datapack search true ending --limit <limit>`.
fn search_with_limit(limit: &str) -> (DatapackEnvironment, Output) {
    let environment = serving_the_search(&[]);
    let output = search(&environment, &["--limit", limit]);
    (environment, output)
}

/// dsrch-1: the datapacks are written one per line in the order Modrinth returned them.
#[test]
fn found_datapacks_are_written_in_modrinths_order() {
    let (_environment, output) = search_with_saved_responses();

    let identifiers: Vec<&str> = output
        .stdout
        .lines()
        .map(|line| line.split('\t').next().unwrap())
        .collect();
    assert_eq!(
        identifiers,
        FOUND.map(|slug| format!("mism: modrinth:{slug}"))
    );
}

/// dsrch-1: a successful search exits with status 0.
#[test]
fn search_exits_with_status_0() {
    let (_environment, output) = search_with_saved_responses();

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dsrch-1: without `--limit`, mism asks Modrinth for 10 datapacks.
#[test]
fn limit_is_10_without_the_option() {
    let (environment, _output) = search_with_saved_responses();

    assert_eq!(
        environment.modrinth.query_parameter("/v2/search", "limit"),
        Some("10".to_owned())
    );
}

/// dsrch-1: mism asks Modrinth for as many datapacks as `--limit` says.
#[test]
fn limit_is_the_number_given() {
    let (environment, _output) = search_with_limit("25");

    assert_eq!(
        environment.modrinth.query_parameter("/v2/search", "limit"),
        Some("25".to_owned())
    );
}

/// dsrch-1: mism searches Modrinth for datapacks only.
#[test]
fn search_is_limited_to_datapacks() {
    let (environment, _output) = search_with_saved_responses();

    assert_eq!(
        environment.modrinth.query_parameter("/v2/search", "facets"),
        Some(r#"[["project_type:datapack"]]"#.to_owned())
    );
}

/// dsrch-1: mism searches Modrinth for the term given.
#[test]
fn search_is_for_the_term_given() {
    let (environment, _output) = search_with_saved_responses();

    assert_eq!(
        environment.modrinth.query_parameter("/v2/search", "query"),
        Some(TERM.to_owned())
    );
}

/// dsrch-2: the example of the specification, True Ending on 2026-10-21, whose newest version
/// is a mod's.
#[test]
fn record_of_a_datapack_has_its_newest_datapack_version() {
    let (_environment, output) = search_with_saved_responses();

    assert_eq!(
        line_of(&output.stdout, "true-ending"),
        Some("modrinth:true-ending\tTrue Ending - Ender Dragon Overhaul\t1.1.4d\t26.1.2")
    );
}

/// dsrch-2: the newest datapack version is chosen also when the list has an older one first.
#[test]
fn newest_datapack_version_is_chosen_wherever_it_is_listed() {
    let output = search_with_versions_changed("true-ending", |versions| {
        versions.as_array_mut().unwrap().reverse();
    });

    assert_eq!(
        line_of(&output.stdout, "true-ending"),
        Some("modrinth:true-ending\tTrue Ending - Ender Dragon Overhaul\t1.1.4d\t26.1.2")
    );
}

/// dsrch-2: the record of each datapack the saved search found.
#[test]
fn records_of_the_found_datapacks() {
    let (_environment, output) = search_with_saved_responses();

    assert_eq!(
        output.stdout,
        "mism: modrinth:true-ending\tTrue Ending - Ender Dragon Overhaul\t1.1.4d\t26.1.2\n\
         mism: modrinth:stellarity_x_true_ending_patch\tStellarity x True Ending ~ Patch\t1.0.0\t26.1.2\n\
         mism: modrinth:vanilla-true-ending\tVanilla True Ending\t1.0\t26.2\n"
    );
}

/// dsrch-2: a newer version whose software lacks `datapack` is not chosen.
#[test]
fn newer_mod_version_is_not_chosen() {
    let output = search_with_versions_changed("true-ending", |versions| {
        versions[0]["version_number"] = "9.9.9".into();
        versions[0]["game_versions"] = json!(["26.3"]);
    });

    assert_eq!(
        line_of(&output.stdout, "true-ending"),
        Some("modrinth:true-ending\tTrue Ending - Ender Dragon Overhaul\t1.1.4d\t26.1.2")
    );
}

/// dsrch-2: a version whose software includes `datapack` among others counts.
#[test]
fn version_for_datapack_among_others_is_chosen() {
    let output = search_with_versions_changed("vanilla-true-ending", |versions| {
        versions[0]["loaders"] = json!(["datapack", "minecraft"]);
    });

    assert_eq!(
        line_of(&output.stdout, "vanilla-true-ending"),
        Some("modrinth:vanilla-true-ending\tVanilla True Ending\t1.0\t26.2")
    );
}

/// dsrch-2 (「mism plugin search と mism datapack search の手順」 方法 4): of equally new
/// datapack versions, the first in the version list is chosen.
#[test]
fn first_of_equally_new_versions_is_chosen() {
    let output = search_with_versions_changed("true-ending", |versions| {
        // The datapack version 1.1.4c comes after the datapack version 1.1.4d in the list.
        versions[3]["date_published"] = versions[1]["date_published"].clone();
    });

    assert_eq!(
        line_of(&output.stdout, "true-ending"),
        Some("modrinth:true-ending\tTrue Ending - Ender Dragon Overhaul\t1.1.4d\t26.1.2")
    );
}

/// dsrch-2, dp-12: a datapack without a version for datapacks has `-` as its last two values.
#[test]
fn datapack_without_a_datapack_version_has_hyphens() {
    let output = search_with_versions_changed("vanilla-true-ending", |versions| {
        versions[0]["loaders"] = json!(["fabric"]);
    });

    assert_eq!(
        line_of(&output.stdout, "vanilla-true-ending"),
        Some("modrinth:vanilla-true-ending\tVanilla True Ending\t-\t-")
    );
}

/// `mism datapack search true ending` where Modrinth's search finds nothing.
fn search_without_hits() -> Output {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .mount_search(TERM, "datapack", "datapack-no-hit");
    search(&environment, &[])
}

/// dsrch-3: a search that finds nothing writes nothing.
#[test]
fn search_without_hits_writes_nothing() {
    assert_eq!(search_without_hits().stdout, "");
}

/// dsrch-3: a search that finds nothing exits with status 0.
#[test]
fn search_without_hits_exits_with_status_0() {
    assert_eq!(search_without_hits().exit_code, 0);
}

/// `mism datapack search true ending` where Modrinth answers the search with status 500.
fn search_failing_at_the_search() -> Output {
    let environment = DatapackEnvironment::without_server();
    environment.modrinth.mount_status("/v2/search", 500, "{}");
    search(&environment, &[])
}

/// dsrch-4: a failed search is reported as err-9 says.
#[test]
fn failed_search_is_an_error() {
    let output = search_failing_at_the_search();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dsrch-4: a failed search writes no datapack.
#[test]
fn failed_search_writes_no_datapack() {
    let output = search_failing_at_the_search();

    assert!(
        !output.stdout.contains("mism: modrinth:"),
        "stdout: {}",
        output.stdout
    );
}

/// `mism datapack search true ending` where Modrinth answers the version list of the second
/// datapack found with status 500.
fn search_failing_at_a_version_list() -> Output {
    let environment = serving_the_search(&["stellarity_x_true_ending_patch"]);
    environment.modrinth.mount_status(
        &format!(
            "/v2/project/{}/version",
            project_id("stellarity_x_true_ending_patch")
        ),
        500,
        "{}",
    );
    search(&environment, &[])
}

/// dsrch-4: a failed version list stops the search as err-9 says.
#[test]
fn failed_version_list_is_an_error() {
    let output = search_failing_at_a_version_list();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// d-search-2, dsrch-4: a failed version list writes no datapack, not even those listed before.
#[test]
fn failed_version_list_writes_no_datapack() {
    let output = search_failing_at_a_version_list();

    assert!(
        !output.stdout.contains("mism: modrinth:"),
        "stdout: {}",
        output.stdout
    );
}

/// d-search-1
#[test]
fn search_requests_the_search_and_each_version_list_only() {
    let (environment, _output) = search_with_saved_responses();

    let mut expected = vec!["/v2/search".to_owned()];
    expected.extend(FOUND.map(|slug| format!("/v2/project/{}/version", project_id(slug))));
    assert_eq!(environment.modrinth.request_paths(), expected);
}

/// dsrch-5: a limit below 10 is an error.
#[test]
fn limit_below_10_is_an_error() {
    let (_environment, output) = search_with_limit("9");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dsrch-5: a limit above 100 is an error.
#[test]
fn limit_above_100_is_an_error() {
    let (_environment, output) = search_with_limit("101");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dsrch-5: a limit that is not a number is an error.
#[test]
fn limit_that_is_not_a_number_is_an_error() {
    let (_environment, output) = search_with_limit("ten");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dsrch-5: a limit with a sign is not an integer of digits only.
#[test]
fn limit_with_a_sign_is_an_error() {
    let (_environment, output) = search_with_limit("+10");

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dsrch-5 (「mism plugin search と mism datapack search の手順」 方法 1): an invalid limit sends
/// no request.
#[test]
fn invalid_limit_sends_no_request() {
    let (environment, _output) = search_with_limit("101");

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// dsrch-5: 10 is a limit.
#[test]
fn limit_of_10_is_accepted() {
    let (_environment, output) = search_with_limit("10");

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dsrch-5: 100 is a limit.
#[test]
fn limit_of_100_is_accepted() {
    let (_environment, output) = search_with_limit("100");

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}
