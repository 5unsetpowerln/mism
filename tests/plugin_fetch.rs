mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::Signal;

use common::fake_modrinth::{
    LUCKPERMS_FILE_NAME, LUCKPERMS_SOURCE, ServedFile, saved_project, sha256_hex, sha512_hex,
};
use common::mism_process::{Output, RunningMism};
use common::plugin_output::{has_line, last_record};
use common::plugin_server_directory::PluginEnvironment;
use common::slow_jar_server::SlowJarServer;
use common::stdin_connection::StdinConnection;
use common::test_plugin_jar::{jar_with_plugin_yml, plugin_yml};

const EXAMPLE_PATH: &str = "/downloads/Example.jar";

fn example_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("Example", "1.0", "1.21"))
}

fn luckperms_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("LuckPerms", "5.5.71", "1.13"))
}

/// The path of the cached jar in a fetch-5 record.
fn fetched_path(output: &Output) -> PathBuf {
    PathBuf::from(last_record(&output.stdout)[4])
}

/// `mism plugin fetch <url of Example.jar>` without `--yes`, with stdin connected as `stdin`.
fn fetch_example_asking(stdin: StdinConnection) -> (PluginEnvironment, Output) {
    let environment = PluginEnvironment::without_server();
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &example_jar());
    let output = environment.mism_plugin(["fetch", &url]).stdin(stdin).run();
    (environment, output)
}

/// `mism plugin fetch <url of Example.jar> --yes`, with `Renamed\n` waiting on stdin.
fn fetch_example_with_yes() -> Output {
    let environment = PluginEnvironment::without_server();
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &example_jar());
    environment
        .mism_plugin(["fetch", &url, "--yes"])
        .stdin(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()))
        .run()
}

/// The fake Modrinth serves LuckPerms v5.5.71-bukkit with a published SHA-512 that is not the
/// served file's.
fn fetch_luckperms_with_wrong_sha512() -> (PluginEnvironment, Output) {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.mount_project("luckperms");
    environment.modrinth.mount_version_list(
        "luckperms",
        &[ServedFile::new(LUCKPERMS_FILE_NAME, &luckperms_jar())
            .with_published_sha512(&sha512_hex(b"another jar"))],
    );
    let output = environment.fetch(LUCKPERMS_SOURCE);
    (environment, output)
}

/// fetch-1: the downloaded file whose SHA-512 is the published one is stored in the cache
/// directory.
#[test]
fn modrinth_file_is_stored() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert_eq!(fs::read(fetched_path(&output)).unwrap(), luckperms_jar());
}

/// fetch-1: a SHA-512 other than the published one is an error.
#[test]
fn modrinth_file_with_another_sha512_is_an_error() {
    let (_environment, output) = fetch_luckperms_with_wrong_sha512();

    assert!(output.stdout.contains(&format!(
        "but Modrinth publishes {}\n",
        sha512_hex(b"another jar")
    )));
}

/// fetch-1: a SHA-512 other than the published one leaves nothing in the cache directory.
#[test]
fn modrinth_file_with_another_sha512_leaves_nothing() {
    let (environment, _output) = fetch_luckperms_with_wrong_sha512();

    assert_eq!(environment.files_in_cache(), Vec::<PathBuf>::new());
}

/// fetch-2: the jar of a URL is stored without a hash to check.
#[test]
fn url_jar_is_stored() {
    let environment = PluginEnvironment::without_server();
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &example_jar());

    let output = environment.fetch(&url);

    assert_eq!(fs::read(fetched_path(&output)).unwrap(), example_jar());
}

/// fetch-2: the name made from the URL is printed.
#[test]
fn name_made_from_the_url_is_printed() {
    let output = fetch_example_with_yes();

    assert!(has_line(
        &output.stdout,
        "plugin name from the URL: Example"
    ));
}

/// fetch-2: the user is asked for the plugin name.
#[test]
fn plugin_name_is_asked() {
    let (_environment, output) = fetch_example_asking(StdinConnection::ClosedAfter(Vec::new()));

    assert!(
        output
            .stdout
            .contains("\nmism: plugin name (empty for Example): ")
    );
}

/// fetch-2: the answer on a terminal is the plugin name.
#[test]
fn answer_on_a_terminal_is_the_plugin_name() {
    let (_environment, output) =
        fetch_example_asking(StdinConnection::Terminal(b"Renamed\n".to_vec()));

    assert_eq!(last_record(&output.stdout)[0], "Renamed");
}

/// fetch-2: an empty answer keeps the name made from the URL.
#[test]
fn empty_answer_keeps_the_name_made_from_the_url() {
    let (_environment, output) = fetch_example_asking(StdinConnection::Terminal(b"\n".to_vec()));

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// fetch-2: stdin that is not a terminal and gives no input keeps the name made from the URL.
#[test]
fn no_input_keeps_the_name_made_from_the_url() {
    let (_environment, output) = fetch_example_asking(StdinConnection::ClosedAfter(Vec::new()));

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// fetch-2: the answer is read from stdin that is not a terminal too.
#[test]
fn answer_on_a_pipe_is_the_plugin_name() {
    let (_environment, output) =
        fetch_example_asking(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()));

    assert_eq!(last_record(&output.stdout)[0], "Renamed");
}

/// fetch-2: with `--yes` the user is not asked.
#[test]
fn yes_keeps_the_name_from_being_asked() {
    let output = fetch_example_with_yes();

    assert!(!output.stdout.contains("plugin name (empty for"));
}

/// fetch-2: with `--yes` the name made from the URL is used.
#[test]
fn yes_uses_the_name_made_from_the_url() {
    let output = fetch_example_with_yes();

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// fetch-2: the jar file name is the plugin name and `.jar`.
#[test]
fn jar_file_name_is_the_plugin_name_and_jar() {
    let (_environment, output) =
        fetch_example_asking(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()));

    assert_eq!(
        fetched_path(&output).file_name().unwrap().to_str(),
        Some("Renamed.jar")
    );
}

/// fetch-2: `mism plugin inspect` names a plugin from a URL the same way.
#[test]
fn inspect_names_a_plugin_from_a_url_the_same_way() {
    let environment = PluginEnvironment::without_server();
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &example_jar());

    let output = environment
        .mism_plugin(["inspect", &url])
        .stdin(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()))
        .run();

    // The value follows the question on its line, as the answer came through a pipe.
    assert!(output.stdout.contains("mism: name\tRenamed\n"));
}

/// d-name-2: the answer `a/b` is an error.
#[test]
fn answer_with_a_slash_is_an_error() {
    let (_environment, output) =
        fetch_example_asking(StdinConnection::ClosedAfter(b"a/b\n".to_vec()));

    assert!(output.stdout.contains(
        "mism: the jar file name \"a/b.jar\" cannot be used: it is empty, `.`, `..`, or contains \
         `/` or NUL\n"
    ));
}

/// d-name-2: the answer `a/b` stores nothing.
#[test]
fn answer_with_a_slash_stores_nothing() {
    let (environment, _output) =
        fetch_example_asking(StdinConnection::ClosedAfter(b"a/b\n".to_vec()));

    assert_eq!(environment.files_in_cache(), Vec::<PathBuf>::new());
}

/// fetch-3: the local jar is copied into the cache directory.
#[test]
fn local_jar_is_copied() {
    let environment = PluginEnvironment::without_server();
    let jar = environment.write_local_jar("Example.jar", &example_jar());

    let output = environment.fetch(jar.to_str().unwrap());

    assert_eq!(fs::read(fetched_path(&output)).unwrap(), example_jar());
}

/// fetch-3: copying leaves the local jar where it was.
#[test]
fn local_jar_stays() {
    let environment = PluginEnvironment::without_server();
    let jar = environment.write_local_jar("Example.jar", &example_jar());

    environment.fetch(jar.to_str().unwrap());

    assert_eq!(fs::read(&jar).unwrap(), example_jar());
}

/// The same jar, first fetched as the local `A.jar`, then from a URL as `B`.
fn fetch_the_same_jar_twice() -> (PluginEnvironment, Output, Output) {
    let environment = PluginEnvironment::without_server();
    let jar = environment.write_local_jar("A.jar", &example_jar());
    let url = environment
        .modrinth
        .mount_jar("/downloads/B.jar", &example_jar());
    let first = environment.fetch(jar.to_str().unwrap());
    let second = environment.fetch(&url);
    (environment, first, second)
}

/// fetch-4: the message names the cached jar's plugin name and path.
#[test]
fn same_jar_is_reported_with_the_cached_name_and_path() {
    let (_environment, first, second) = fetch_the_same_jar_twice();

    assert!(has_line(
        &second.stdout,
        &format!(
            "the jar is the same as the cached jar of A: {}",
            fetched_path(&first).display()
        )
    ));
}

/// fetch-4: no second jar is stored.
#[test]
fn same_jar_is_not_stored_again() {
    let (environment, _first, _second) = fetch_the_same_jar_twice();

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(output.stdout.lines().count(), 1);
}

/// fetch-4: the cached jar and its plugin name are the result.
#[test]
fn same_jar_keeps_the_cached_name_and_path() {
    let (_environment, first, second) = fetch_the_same_jar_twice();
    let second_record = last_record(&second.stdout);

    assert_eq!(
        (second_record[0], PathBuf::from(second_record[4])),
        ("A", fetched_path(&first))
    );
}

/// fetch-4: the fetch of the same jar succeeds.
#[test]
fn fetch_of_the_same_jar_succeeds() {
    let (_environment, _first, second) = fetch_the_same_jar_twice();

    assert_eq!(second.exit_code, 0);
}

/// LuckPerms v5.5.71-bukkit fetched first from a URL, then from Modrinth with the same content,
/// in a server directory for `place`. Returns the URL too.
fn fetch_from_a_url_then_from_modrinth() -> (PluginEnvironment, String) {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    let url = environment.modrinth.mount_jar(
        &format!("/downloads/{LUCKPERMS_FILE_NAME}"),
        &luckperms_jar(),
    );
    environment.modrinth.serve_luckperms(&luckperms_jar());
    environment.fetch(&url);
    environment.fetch(LUCKPERMS_SOURCE);
    (environment, url)
}

/// fetch-4, list-4: `mism plugin list --global` prints the later fetch's source and version
/// number with the earlier plugin name.
#[test]
fn list_shows_the_later_source_of_the_same_jar() {
    let (environment, _url) = fetch_from_a_url_then_from_modrinth();

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: LuckPerms-Bukkit-5.5.71\tv5.5.71-bukkit\t{}\t{LUCKPERMS_SOURCE}\n",
            sha256_hex(&luckperms_jar())
        )
    );
}

/// fetch-4: `mism plugin place` finds the jar by the later source.
#[test]
fn place_finds_the_same_jar_by_the_later_source() {
    let (environment, _url) = fetch_from_a_url_then_from_modrinth();

    let output = environment.mism_plugin(["place", LUCKPERMS_SOURCE]).run();

    assert_eq!(output.exit_code, 0);
}

/// fetch-4: `mism plugin place` does not find the jar by the earlier URL source.
#[test]
fn place_does_not_find_the_same_jar_by_the_earlier_url() {
    let (environment, url) = fetch_from_a_url_then_from_modrinth();

    let output = environment.mism_plugin(["place", &url]).run();

    assert!(has_line(
        &output.stdout,
        &format!("the cache directory has no jar fetched from {url}")
    ));
}

/// fetch-4: `mism plugin place` finds the jar by a local jar file with the same content.
#[test]
fn place_finds_the_same_jar_by_a_local_jar() {
    let (environment, _url) = fetch_from_a_url_then_from_modrinth();
    let jar = environment.write_local_jar(LUCKPERMS_FILE_NAME, &luckperms_jar());

    let output = environment
        .mism_plugin(["place".as_ref(), jar.as_os_str()])
        .run();

    assert_eq!(output.exit_code, 0);
}

/// fetch-5: a fetch from Modrinth prints the plugin name, the version number, the source, and
/// the SHA-256.
#[test]
fn modrinth_fetch_prints_its_record() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert_eq!(
        last_record(&output.stdout)[..4],
        [
            "LuckPerms",
            "v5.5.71-bukkit",
            LUCKPERMS_SOURCE,
            sha256_hex(&luckperms_jar()).as_str()
        ]
    );
}

/// fetch-5, plg-9: a fetch from a URL prints `-` as the version number and the URL as given.
#[test]
fn url_fetch_prints_its_record() {
    let environment = PluginEnvironment::without_server();
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &example_jar());

    let output = environment.fetch(&url);

    assert_eq!(
        last_record(&output.stdout)[..4],
        [
            "Example",
            "-",
            url.as_str(),
            sha256_hex(&example_jar()).as_str()
        ]
    );
}

/// fetch-5: a fetch of a local jar prints the path as given as the source.
#[test]
fn local_fetch_prints_the_path_as_given() {
    let environment = PluginEnvironment::without_server();
    environment.write_local_jar("Example.jar", &example_jar());

    let output = environment.fetch("../outside/Example.jar");

    assert_eq!(last_record(&output.stdout)[2], "../outside/Example.jar");
}

/// fetch-5: the last value is the absolute path of the jar in the cache directory.
#[test]
fn printed_path_is_absolute_in_the_cache_directory() {
    let environment = PluginEnvironment::without_server();
    environment.write_local_jar("Example.jar", &example_jar());

    let output = environment.fetch("../outside/Example.jar");

    assert!(fetched_path(&output).starts_with(environment.cache()));
}

/// fetch-5: a successful fetch exits with status 0.
#[test]
fn successful_fetch_exits_with_status_0() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.fetch(LUCKPERMS_SOURCE);

    assert_eq!(output.exit_code, 0);
}

/// Waits until `path` exists, failing the test after 30 seconds.
fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} did not appear within 30 seconds",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// Starts fetching from a server that sends a jar in 50 pieces 100 ms apart, and waits until
/// mism has created the cache directory, which it does right before the download.
fn start_a_slow_download(environment: &PluginEnvironment) -> (RunningMism, SlowJarServer) {
    let jar_server = SlowJarServer::start(&example_jar(), 50, Duration::from_millis(100));
    let mism = environment
        .mism_plugin(["fetch", jar_server.url(), "--yes"])
        .spawn();
    wait_for_path(&environment.cache());
    (mism, jar_server)
}

/// Sends `SIGINT` to mism during a slow download.
fn interrupt_a_download() -> (PluginEnvironment, Output) {
    let environment = PluginEnvironment::without_server();
    let (mism, _jar_server) = start_a_slow_download(&environment);
    mism.send(Signal::SIGINT);
    let output = mism.wait();
    (environment, output)
}

/// d-fetch-1: SIGINT during the download ends mism with status 130.
#[test]
fn interrupted_download_exits_with_130() {
    let (_environment, output) = interrupt_a_download();

    assert_eq!(output.exit_code, 130);
}

/// d-fetch-1: SIGINT during the download adds no SHA-256 directory.
#[test]
fn interrupted_download_adds_no_cached_jar() {
    let (environment, _output) = interrupt_a_download();

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(output.stdout, "");
}

/// The directories below the cache directory named by 64 lowercase hexadecimal digits.
fn sha256_directories(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            let name = path.file_name().unwrap().to_str().unwrap();
            if name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                found.push(path.clone());
            }
            found.extend(sha256_directories(&path));
        }
    }
    found
}

/// d-pcache-1: mism killed during a download leaves no SHA-256 directory with only one of its
/// two files.
#[test]
fn killed_download_leaves_no_sha256_directory() {
    let environment = PluginEnvironment::without_server();
    let (mism, _jar_server) = start_a_slow_download(&environment);

    mism.kill();

    assert_eq!(
        sha256_directories(&environment.cache()),
        Vec::<PathBuf>::new()
    );
}

/// LuckPerms fetched from a fake Modrinth whose project is titled `A/B`.
fn fetch_plugin_titled_a_slash_b() -> (PluginEnvironment, Output) {
    let environment = PluginEnvironment::without_server();
    let mut project = saved_project("luckperms");
    project["title"] = "A/B".into();
    environment
        .modrinth
        .mount_project_json("luckperms", &project);
    environment.modrinth.mount_version_list(
        "luckperms",
        &[ServedFile::new(LUCKPERMS_FILE_NAME, &luckperms_jar())],
    );
    let output = environment.fetch(LUCKPERMS_SOURCE);
    (environment, output)
}

/// d-pcache-3: the SHA-256 directory of the plugin `A/B` is in `plugins/A-B/`.
#[test]
fn slash_in_the_plugin_name_becomes_a_hyphen_in_the_directory() {
    let (environment, output) = fetch_plugin_titled_a_slash_b();

    assert_eq!(
        fetched_path(&output).parent().unwrap().parent(),
        Some(environment.cache().join("plugins/A-B").as_path())
    );
}

/// d-pcache-3: its `source.toml` records the name `A/B`.
#[test]
fn source_toml_keeps_the_slash_in_the_name() {
    let (_environment, output) = fetch_plugin_titled_a_slash_b();
    let source_toml = fetched_path(&output).with_file_name("source.toml");

    let table: toml::Table = fs::read_to_string(source_toml).unwrap().parse().unwrap();

    assert_eq!(table["name"].as_str(), Some("A/B"));
}
