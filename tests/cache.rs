mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::fake_java::Behavior;
use common::fake_papermc::sha256_hex;
use common::mism_process::{MismProcess, Output};
use common::new_server_environment::{JAR_BYTES, NewServerEnvironment};

const JAR_NAME: &str = "paper-26.3-206.jar";

/// `mism new paper <target>` with `XDG_CACHE_HOME` set to the environment's.
fn new_with_xdg(environment: &NewServerEnvironment, target: &Path, extra: &[&str]) -> MismProcess {
    environment
        .mism_new(["paper".as_ref(), target.as_os_str()])
        .arguments(extra)
        .environment("XDG_CACHE_HOME", environment.xdg_cache_home())
}

/// `<cache>/paper/<minecraft version>/<software version>`, the directory of one cached jar.
fn cache_directory(cache: &Path, minecraft_version: &str, software_version: u64) -> PathBuf {
    cache
        .join("mism/paper")
        .join(minecraft_version)
        .join(software_version.to_string())
}

fn place_cached_jar(directory: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    fs::create_dir_all(directory).unwrap();
    let path = directory.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

/// Every file below `directory`, at any depth; empty when `directory` does not exist.
fn files_below(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_below(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

fn succeeded(output: &Output) -> bool {
    output.exit_code == 0
}

/// cache-1
#[test]
fn jar_is_cached_below_xdg_cache_home() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);

    new_with_xdg(&environment, &environment.server_path(), &[]).run();

    let jar = cache_directory(&environment.xdg_cache_home(), "26.3", 206).join(JAR_NAME);
    assert_eq!(fs::read(jar).unwrap(), JAR_BYTES);
}

/// cache-2
#[test]
fn jar_is_cached_below_home_dot_cache_without_xdg_cache_home() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);

    environment
        .mism_new(["paper".as_ref(), environment.server_path().as_os_str()])
        .run();

    let jar = cache_directory(&environment.home().join(".cache"), "26.3", 206).join(JAR_NAME);
    assert_eq!(fs::read(jar).unwrap(), JAR_BYTES);
}

/// cache-3
#[test]
fn cached_jar_with_the_published_sha256_is_not_downloaded() {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("paper");
    let jar_url = environment.papermc.mount_jar_download(
        "paper",
        "26.3",
        206,
        JAR_BYTES,
        &sha256_hex(JAR_BYTES),
    );
    let directory = cache_directory(&environment.xdg_cache_home(), "26.3", 206);
    place_cached_jar(&directory, JAR_NAME, JAR_BYTES);

    let output = new_with_xdg(&environment, &environment.server_path(), &[]).run();

    assert_eq!(
        (
            succeeded(&output),
            environment.papermc.request_count_for(&jar_url)
        ),
        (true, 0)
    );
}

/// cache-4: the jar is requested once.
#[test]
fn cached_jar_with_another_sha256_is_downloaded() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let directory = cache_directory(&environment.xdg_cache_home(), "26.3", 206);
    place_cached_jar(&directory, JAR_NAME, b"a corrupted jar");

    new_with_xdg(&environment, &environment.server_path(), &[]).run();

    let jar_url = format!(
        "{}/jars/{JAR_NAME}",
        environment.papermc.base_url().trim_end_matches("/v3")
    );
    assert_eq!(environment.papermc.request_count_for(&jar_url), 1);
}

/// cache-4: the cached file has the served content afterwards.
#[test]
fn cached_jar_with_another_sha256_is_replaced() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let directory = cache_directory(&environment.xdg_cache_home(), "26.3", 206);
    let jar = place_cached_jar(&directory, JAR_NAME, b"a corrupted jar");

    new_with_xdg(&environment, &environment.server_path(), &[]).run();

    assert_eq!(fs::read(jar).unwrap(), JAR_BYTES);
}

fn serve_jar_with_wrong_published_sha256() -> (NewServerEnvironment, Output) {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("paper");
    environment.papermc.mount_jar_download(
        "paper",
        "26.3",
        206,
        JAR_BYTES,
        &sha256_hex(b"some other jar"),
    );
    let output = new_with_xdg(&environment, &environment.server_path(), &[]).run();
    (environment, output)
}

/// cache-5: exit status 1
#[test]
fn jar_whose_sha256_differs_exits_with_status_1() {
    let (_environment, output) = serve_jar_with_wrong_published_sha256();

    assert_eq!(output.exit_code, 1);
}

/// cache-5: stdout reports the mismatch
#[test]
fn jar_whose_sha256_differs_is_reported() {
    let (_environment, output) = serve_jar_with_wrong_published_sha256();

    assert!(
        output
            .stdout
            .lines()
            .any(|line| line.starts_with("mism: the SHA-256 of ")
                && line.contains(&format!(
                    "but PaperMC publishes {}",
                    sha256_hex(b"some other jar")
                ))),
        "stdout: {}",
        output.stdout
    );
}

/// d-cache-1: after the mismatch no file is left in the cache.
#[test]
fn no_file_is_cached_after_a_sha256_mismatch() {
    let (environment, _output) = serve_jar_with_wrong_published_sha256();

    let directory = cache_directory(&environment.xdg_cache_home(), "26.3", 206);
    assert_eq!(files_below(&directory), Vec::<PathBuf>::new());
}

/// cache-6
#[test]
fn jars_of_other_versions_remain_in_the_cache() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);
    let other_jar = b"pretend this is paper-26.2-134.jar";
    environment
        .papermc
        .mount_jar_download("paper", "26.2", 134, other_jar, &sha256_hex(other_jar));
    new_with_xdg(&environment, &environment.servers().join("first"), &[]).run();

    new_with_xdg(
        &environment,
        &environment.servers().join("second"),
        &["--version", "26.2"],
    )
    .run();

    let cache = environment.xdg_cache_home();
    let jars = [
        cache_directory(&cache, "26.3", 206).join(JAR_NAME),
        cache_directory(&cache, "26.2", 134).join("paper-26.2-134.jar"),
    ];
    assert_eq!(jars.map(|jar| jar.is_file()), [true, true]);
}

/// d-cache-2
#[test]
fn cached_jar_has_the_published_sha256() {
    let environment = NewServerEnvironment::serving_paper_26_3(Behavior::Normal);

    new_with_xdg(&environment, &environment.server_path(), &[]).run();

    let jar = cache_directory(&environment.xdg_cache_home(), "26.3", 206).join(JAR_NAME);
    assert_eq!(sha256_hex(&fs::read(jar).unwrap()), sha256_hex(JAR_BYTES));
}

/// d-cache-3: both processes succeed.
#[test]
fn simultaneous_downloads_of_one_jar_both_succeed() {
    let (outputs, _environment) = download_simultaneously();

    assert_eq!(outputs.map(|output| output.exit_code), [0, 0]);
}

/// d-cache-3: exactly one file remains in the cache directory of the version.
#[test]
fn simultaneous_downloads_of_one_jar_leave_one_file() {
    let (_outputs, environment) = download_simultaneously();

    let directory = cache_directory(&environment.home().join(".cache"), "26.3", 206);
    assert_eq!(files_below(&directory), vec![directory.join(JAR_NAME)]);
}

/// Runs two `mism new` processes at once for the same jar, which the fake PaperMC serves slowly
/// so that the downloads overlap.
fn download_simultaneously() -> ([Output; 2], NewServerEnvironment) {
    let environment = NewServerEnvironment::new(Behavior::Normal);
    environment.papermc.mount_version_list_from_fixture("paper");
    environment.papermc.mount_jar_download_after(
        "paper",
        "26.3",
        206,
        JAR_BYTES,
        &sha256_hex(JAR_BYTES),
        Duration::from_millis(500),
    );
    let processes = ["first", "second"].map(|name| {
        environment
            .mism_new([
                "paper".as_ref(),
                environment.servers().join(name).as_os_str(),
            ])
            .spawn()
    });
    let outputs = processes.map(|process| process.wait());
    (outputs, environment)
}
