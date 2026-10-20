mod common;

use common::fake_papermc::{self, FakePaperMc};
use common::mism_process::{MismProcess, Output};
use common::temporary_directory;

fn run_versions(software: &str, papermc_url: &str) -> Output {
    let home = temporary_directory::create();
    MismProcess::new()
        .arguments(["versions", software])
        .environment("HOME", home.path())
        .environment("MISM_PAPERMC_URL", papermc_url)
        .run()
}

fn run_versions_against_fixture(software: &str) -> (Output, FakePaperMc) {
    let papermc = FakePaperMc::start();
    papermc.mount_version_list_from_fixture(software);
    let output = run_versions(software, &papermc.base_url());
    (output, papermc)
}

/// The Minecraft version of each line of stdout, which is `mism: `, the version, a tab, and the
/// support status.
fn version_ids(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .map(|line| {
            let listed = line
                .strip_prefix("mism: ")
                .unwrap_or_else(|| panic!("the line lacks `mism: `: {line}"));
            listed.split('\t').next().unwrap()
        })
        .collect()
}

fn entry(id: &str) -> String {
    format!(
        r#"{{"version":{{"id":"{id}","support":{{"status":"SUPPORTED"}},"java":{{"version":{{"minimum":21}}}}}},"builds":[1]}}"#
    )
}

fn fixture_ids(software: &str) -> Vec<String> {
    fake_papermc::saved_version_list(software)["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["version"]["id"].as_str().unwrap().to_owned())
        .collect()
}

/// ver-1
#[test]
fn paper_lists_all_minecraft_versions_newest_first() {
    let (output, _papermc) = run_versions_against_fixture("paper");

    // The fixture is stored newest first, as PaperMC returns it.
    assert_eq!(version_ids(&output.stdout), fixture_ids("paper"));
}

/// ver-1
#[test]
fn folia_lists_all_minecraft_versions_newest_first() {
    let (output, _papermc) = run_versions_against_fixture("folia");

    assert_eq!(version_ids(&output.stdout), fixture_ids("folia"));
}

/// ver-1
#[test]
fn successful_listing_exits_with_status_0() {
    let (output, _papermc) = run_versions_against_fixture("paper");

    assert_eq!(output.exit_code, 0);
}

/// ver-2
#[test]
fn each_line_is_the_id_a_tab_and_the_support_status() {
    let (output, _papermc) = run_versions_against_fixture("folia");

    assert!(
        output.stdout.starts_with(
            "mism: 26.3\tSUPPORTED\nmism: 26.2\tDEPRECATED\nmism: 26.1.2\tUNSUPPORTED\n"
        )
    );
}

/// ver-3
#[test]
fn unreachable_server_prints_nothing_to_stdout() {
    let output = run_versions("paper", &fake_papermc::unreachable_base_url());

    assert_eq!(output.stdout, "");
}

/// ver-3
#[test]
fn unreachable_server_exits_with_status_1() {
    let output = run_versions("paper", &fake_papermc::unreachable_base_url());

    assert_eq!(output.exit_code, 1);
}

/// ver-3
#[test]
fn unreachable_server_reports_an_error() {
    let output = run_versions("paper", &fake_papermc::unreachable_base_url());

    assert!(!output.stderr.is_empty());
}

/// d-versions-1
#[test]
fn versions_calls_the_api_once() {
    let (_output, papermc) = run_versions_against_fixture("paper");

    assert_eq!(papermc.request_count(), 1);
}

/// d-api-3
#[test]
fn list_in_the_wrong_order_is_printed_newest_first() {
    let papermc = FakePaperMc::start();
    let body = format!(
        r#"{{"versions":[{},{},{},{}]}}"#,
        entry("1.20.4"),
        entry("26.1"),
        entry("1.21.11"),
        entry("1.9"),
    );
    papermc.mount_version_list("paper", &body);

    let output = run_versions("paper", &papermc.base_url());

    assert_eq!(
        version_ids(&output.stdout),
        ["26.1", "1.21.11", "1.20.4", "1.9"]
    );
}

/// d-cli-1
#[test]
fn error_output_starts_with_mism_prefix() {
    let output = run_versions("paper", &fake_papermc::unreachable_base_url());

    assert!(output.stderr.starts_with("mism: "));
}

/// d-cli-2
#[test]
fn each_level_of_the_error_chain_has_a_caused_by_line() {
    // The chain is: request failed (HTTP client) -> the ureq error.
    let output = run_versions("paper", &fake_papermc::unreachable_base_url());

    let depth = output
        .stderr
        .lines()
        .filter(|line| line.starts_with("mism: caused by: "))
        .count();
    assert_eq!(depth, 1);
}

/// The real-API test of the design's test section.
#[test]
fn real_api_lists_paper_versions() {
    if std::env::var("MISM_TEST_REAL_API").as_deref() != Ok("1") {
        return;
    }
    let home = temporary_directory::create();

    let output = MismProcess::new()
        .arguments(["versions", "paper"])
        .environment("HOME", home.path())
        .run();

    assert_eq!(
        (output.exit_code, output.stdout.lines().count() > 0),
        (0, true),
        "stderr: {}",
        output.stderr
    );
}
