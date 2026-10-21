mod common;

use serde_json::Value;

use common::fake_modrinth::{project_id, saved_project, saved_search, saved_version_list};
use common::mism_process::{MismProcess, Output};
use common::plugin_output::has_line;
use common::plugin_server_directory::PluginEnvironment;

const TERM: &str = "multiverse portals";
/// The identifiers the saved search for `multiverse portals` found, in Modrinth's order.
const FOUND: [&str; 5] = [
    "multiverse-portals",
    "multiverse-netherportals",
    "multiverse-signportals",
    "locationlast",
    "playerportals",
];

/// An environment whose fake Modrinth serves the saved search for `multiverse portals` and the
/// saved version list of each plugin it found, except those that `serve_instead` serves.
fn serving_the_search(serve_instead: &[&str]) -> PluginEnvironment {
    let environment = PluginEnvironment::without_server();
    environment
        .modrinth
        .mount_search(TERM, "multiverse-portals");
    for slug in FOUND
        .into_iter()
        .filter(|slug| !serve_instead.contains(slug))
    {
        environment.modrinth.mount_version_list(slug, &[]);
    }
    environment
}

fn search(environment: &PluginEnvironment, extra: &[&str]) -> Output {
    environment
        .mism_plugin(["search", TERM])
        .arguments(extra)
        .run()
}

/// `mism plugin search multiverse portals` with the saved responses.
fn search_with_saved_responses() -> (PluginEnvironment, Output) {
    let environment = serving_the_search(&[]);
    let output = search(&environment, &[]);
    (environment, output)
}

/// `mism plugin search multiverse portals` with the version list of PlayerPortals replaced by
/// `change` applied to its saved one.
fn search_with_playerportals_changed(change: impl FnOnce(&mut Value)) -> Output {
    let environment = serving_the_search(&["playerportals"]);
    let mut versions = saved_version_list("playerportals");
    change(&mut versions);
    environment
        .modrinth
        .mount_version_list_json("playerportals", &versions);
    search(&environment, &[])
}

/// The line mism wrote about the plugin `identifier`, without `mism: `.
fn line_of<'a>(stdout: &'a str, identifier: &str) -> Option<&'a str> {
    let start = format!("mism: modrinth:{identifier}\t");
    stdout
        .lines()
        .find(|line| line.starts_with(&start))
        .map(|line| &line["mism: ".len()..])
}

/// The saved search response with its hits replaced by one hit of LuckPerms, made from the saved
/// project response: the saved search for `multiverse portals` did not find LuckPerms.
fn search_finding_luckperms() -> Value {
    let mut results = saved_search("multiverse-portals");
    let project = saved_project("luckperms");
    let mut hit = results["hits"][0].clone();
    for field in ["slug", "title"] {
        hit[field] = project[field].clone();
    }
    hit["project_id"] = project["id"].clone();
    results["hits"] = Value::Array(vec![hit]);
    results
}

/// `mism plugin search luckperms`, which finds LuckPerms with its saved version list, changed
/// by `change`.
fn search_luckperms(change: impl FnOnce(&mut Value)) -> Output {
    let environment = PluginEnvironment::without_server();
    environment
        .modrinth
        .mount_search_json("luckperms", &search_finding_luckperms());
    let mut versions = saved_version_list("luckperms");
    change(&mut versions);
    environment
        .modrinth
        .mount_version_list_json("luckperms", &versions);
    environment.mism_plugin(["search", "luckperms"]).run()
}

/// `mism plugin search multiverse portals --limit <limit>`.
fn search_with_limit(limit: &str) -> (PluginEnvironment, Output) {
    let environment = serving_the_search(&[]);
    let output = search(&environment, &["--limit", limit]);
    (environment, output)
}

/// srch-1: the plugins are written one per line in the order Modrinth returned them.
#[test]
fn found_plugins_are_written_in_modrinths_order() {
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

/// srch-1: a successful search exits with status 0.
#[test]
fn search_exits_with_status_0() {
    let (_environment, output) = search_with_saved_responses();

    assert_eq!(output.exit_code, 0);
}

/// srch-1: without `--limit`, mism asks Modrinth for 10 plugins.
#[test]
fn limit_is_10_without_the_option() {
    let (environment, _output) = search_with_saved_responses();

    assert_eq!(
        environment.modrinth.query_parameter("/v2/search", "limit"),
        Some("10".to_owned())
    );
}

/// srch-1: mism asks Modrinth for as many plugins as `--limit` says.
#[test]
fn limit_is_the_number_given() {
    let (environment, _output) = search_with_limit("25");

    assert_eq!(
        environment.modrinth.query_parameter("/v2/search", "limit"),
        Some("25".to_owned())
    );
}

/// srch-1: mism searches Modrinth for plugins only.
#[test]
fn search_is_limited_to_plugins() {
    let (environment, _output) = search_with_saved_responses();

    assert_eq!(
        environment.modrinth.query_parameter("/v2/search", "facets"),
        Some(r#"[["project_type:plugin"]]"#.to_owned())
    );
}

/// srch-2: the example of the specification, LuckPerms on 2026-10-21, whose newest version is
/// for NeoForge.
#[test]
fn record_of_a_plugin_has_its_newest_version_for_paper_or_folia() {
    let output = search_luckperms(|_| {});

    assert_eq!(
        output.stdout,
        "mism: modrinth:luckperms\tLuckPerms\tv5.5.71-bukkit\tbukkit,folia,paper,spigot\t26.3\n"
    );
}

/// srch-2: the record of each plugin the saved search found.
#[test]
fn records_of_the_found_plugins() {
    let (_environment, output) = search_with_saved_responses();

    assert_eq!(
        output.stdout,
        "mism: modrinth:multiverse-portals\tMultiverse-Portals\t5.3.0\tbukkit,paper,purpur,spigot\t26.3\n\
         mism: modrinth:multiverse-netherportals\tMultiverse-NetherPortals\t5.1.0\tbukkit,paper,purpur,spigot\t26.3\n\
         mism: modrinth:multiverse-signportals\tMultiverse-SignPortals\t5.0.4\tbukkit,paper,purpur,spigot\t26.3\n\
         mism: modrinth:locationlast\tLastLocation\t1.1\tbukkit,paper,spigot\t26.2\n\
         mism: modrinth:playerportals\tPlayerPortals\t1.1\tbukkit,paper,purpur,spigot\t26.1.2\n"
    );
}

/// srch-2: a version whose server software is Folia alone counts.
#[test]
fn version_for_folia_alone_is_chosen() {
    let output = search_luckperms(|versions| {
        versions[0]["loaders"] = serde_json::json!(["folia"]);
    });

    assert_eq!(
        line_of(&output.stdout, "luckperms"),
        Some("modrinth:luckperms\tLuckPerms\tv5.5.87-neoforge\tfolia\t26.3")
    );
}

/// srch-2 (「mism plugin search の手順」 方法 4): of equally new versions, the first in the
/// version list is chosen.
#[test]
fn first_of_equally_new_versions_is_chosen() {
    let output = search_with_playerportals_changed(|versions| {
        versions[1]["date_published"] = versions[0]["date_published"].clone();
        versions[1]["loaders"] = serde_json::json!(["paper"]);
    });

    assert_eq!(
        line_of(&output.stdout, "playerportals"),
        Some("modrinth:playerportals\tPlayerPortals\t1.1\tbukkit,paper,purpur,spigot\t26.1.2")
    );
}

/// srch-2, plg-9: a plugin without a version for Paper or Folia has `-` as its last three values.
#[test]
fn plugin_without_a_version_for_paper_or_folia_has_hyphens() {
    let output = search_with_playerportals_changed(|versions| {
        for version in versions.as_array_mut().unwrap() {
            version["loaders"] = serde_json::json!(["fabric"]);
        }
    });

    assert_eq!(
        line_of(&output.stdout, "playerportals"),
        Some("modrinth:playerportals\tPlayerPortals\t-\t-\t-")
    );
}

/// srch-3: a search that finds nothing writes nothing.
#[test]
fn search_without_hits_writes_nothing() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.mount_search(TERM, "no-hit");

    let output = search(&environment, &[]);

    assert_eq!(output.stdout, "");
}

/// srch-3: a search that finds nothing exits with status 0.
#[test]
fn search_without_hits_exits_with_status_0() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.mount_search(TERM, "no-hit");

    let output = search(&environment, &[]);

    assert_eq!(output.exit_code, 0);
}

/// `mism plugin search multiverse portals` where Modrinth answers the search with status 500.
fn search_failing_at_the_search() -> Output {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.mount_status("/v2/search", 500, "{}");
    search(&environment, &[])
}

/// srch-4: a failed search exits with status 1.
#[test]
fn failed_search_exits_with_status_1() {
    assert_eq!(search_failing_at_the_search().exit_code, 1);
}

/// srch-4: a failed search is reported as err-9 says, with the hint at the network.
#[test]
fn failed_search_hints_at_the_network() {
    assert!(has_line(
        &search_failing_at_the_search().stdout,
        "hint: check your network connection"
    ));
}

/// `mism plugin search multiverse portals` where Modrinth answers the version list of the third
/// plugin found with status 500.
fn search_failing_at_a_version_list() -> Output {
    let environment = serving_the_search(&["multiverse-signportals"]);
    environment.modrinth.mount_status(
        &format!(
            "/v2/project/{}/version",
            project_id("multiverse-signportals")
        ),
        500,
        "{}",
    );
    search(&environment, &[])
}

/// srch-4: a failed version list stops the search with status 1.
#[test]
fn failed_version_list_exits_with_status_1() {
    assert_eq!(search_failing_at_a_version_list().exit_code, 1);
}

/// d-search-2
#[test]
fn failed_version_list_writes_no_plugin() {
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

/// srch-5: a limit below 10 is an error.
#[test]
fn limit_below_10_is_an_error() {
    let (_environment, output) = search_with_limit("9");

    assert!(has_line(
        &output.stdout,
        "--limit \"9\" is not an integer from 10 to 100"
    ));
}

/// srch-5: a limit above 100 is an error.
#[test]
fn limit_above_100_is_an_error() {
    let (_environment, output) = search_with_limit("101");

    assert!(has_line(
        &output.stdout,
        "--limit \"101\" is not an integer from 10 to 100"
    ));
}

/// srch-5: a limit that is not a number is an error.
#[test]
fn limit_that_is_not_a_number_is_an_error() {
    let (_environment, output) = search_with_limit("ten");

    assert!(has_line(
        &output.stdout,
        "--limit \"ten\" is not an integer from 10 to 100"
    ));
}

/// srch-5: a limit with a sign is not an integer of digits only.
#[test]
fn limit_with_a_sign_is_an_error() {
    let (_environment, output) = search_with_limit("+10");

    assert!(has_line(
        &output.stdout,
        "--limit \"+10\" is not an integer from 10 to 100"
    ));
}

/// srch-5: an invalid limit exits with status 1 (err-9).
#[test]
fn invalid_limit_exits_with_status_1() {
    let (_environment, output) = search_with_limit("9");

    assert_eq!(output.exit_code, 1);
}

/// srch-5 (「mism plugin search の手順」 方法 1): an invalid limit sends no request.
#[test]
fn invalid_limit_sends_no_request() {
    let (environment, _output) = search_with_limit("101");

    assert_eq!(environment.modrinth.request_count(), 0);
}

/// srch-5: 10 is a limit.
#[test]
fn limit_of_10_is_accepted() {
    let (_environment, output) = search_with_limit("10");

    assert_eq!(output.exit_code, 0);
}

/// srch-5: 100 is a limit.
#[test]
fn limit_of_100_is_accepted() {
    let (_environment, output) = search_with_limit("100");

    assert_eq!(output.exit_code, 0);
}

/// The real-API test of the design's test section, for Modrinth.
#[test]
fn real_api_finds_luckperms() {
    if std::env::var("MISM_TEST_REAL_API").as_deref() != Ok("1") {
        return;
    }

    let output = MismProcess::new()
        .arguments(["plugin", "search", "luckperms"])
        .run();

    assert_eq!(
        (
            output.exit_code,
            output
                .stdout
                .lines()
                .any(|line| line.starts_with("mism: modrinth:luckperms\t"))
        ),
        (0, true),
        "stdout: {}",
        output.stdout
    );
}
