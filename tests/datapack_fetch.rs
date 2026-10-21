mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::Signal;

use common::datapack_server_directory::DatapackEnvironment;
use common::fake_modrinth::{
    ServedFile, TRUE_ENDING_FILE_NAME, TRUE_ENDING_NAME, TRUE_ENDING_SOURCE, sha256_hex, sha512_hex,
};
use common::management_environment::entries_below;
use common::mism_process::{Output, RunningMism};
use common::plugin_output::{has_line, last_record, reported_an_error};
use common::slow_jar_server::SlowJarServer;
use common::stdin_connection::StdinConnection;
use common::test_datapack_zip::{
    datapack_declaring, datapack_with_pack_mcmeta, true_ending_pack_mcmeta,
};

const EXAMPLE_PATH: &str = "/downloads/Example.zip";

/// A datapack for exactly the datapack format 121, that of the test servers.
fn example_datapack() -> Vec<u8> {
    datapack_declaring(r#""min_format": 121, "max_format": 121"#)
}

fn true_ending_datapack() -> Vec<u8> {
    datapack_with_pack_mcmeta(&true_ending_pack_mcmeta())
}

/// The path of the cached file in a dfetch-5 record.
fn fetched_path(output: &Output) -> PathBuf {
    PathBuf::from(last_record(&output.stdout)[4])
}

/// `mism datapack fetch <url of Example.zip>` without `--yes`, with stdin connected as `stdin`.
fn fetch_example_asking(stdin: StdinConnection) -> (DatapackEnvironment, Output) {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());
    let output = environment
        .mism_datapack(["fetch", &url])
        .stdin(stdin)
        .run();
    (environment, output)
}

/// `mism datapack fetch <url of Example.zip> --yes`, with `Renamed\n` waiting on stdin.
fn fetch_example_with_yes() -> Output {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());
    environment
        .mism_datapack(["fetch", &url, "--yes"])
        .stdin(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()))
        .run()
}

/// The fake Modrinth serves True Ending 1.1.4d with a published SHA-512 that is not the served
/// file's.
fn fetch_true_ending_with_wrong_sha512() -> (DatapackEnvironment, Output) {
    let environment = DatapackEnvironment::without_server();
    environment.modrinth.mount_project("true-ending");
    environment.modrinth.mount_version_list(
        "true-ending",
        &[
            ServedFile::new(TRUE_ENDING_FILE_NAME, &true_ending_datapack())
                .with_published_sha512(&sha512_hex(b"another datapack")),
        ],
    );
    let output = environment.fetch(TRUE_ENDING_SOURCE);
    (environment, output)
}

/// dfetch-1: the downloaded file whose SHA-512 is the published one is stored in the cache
/// directory.
#[test]
fn modrinth_file_is_stored() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    let output = environment.fetch(TRUE_ENDING_SOURCE);

    assert_eq!(
        fs::read(fetched_path(&output)).ok(),
        Some(true_ending_datapack())
    );
}

/// dfetch-1: a SHA-512 other than the published one is an error.
#[test]
fn modrinth_file_with_another_sha512_is_an_error() {
    let (_environment, output) = fetch_true_ending_with_wrong_sha512();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dfetch-1: a SHA-512 other than the published one leaves nothing in the cache directory.
#[test]
fn modrinth_file_with_another_sha512_leaves_nothing() {
    let (environment, _output) = fetch_true_ending_with_wrong_sha512();

    assert_eq!(entries_below(&environment.cache()), Vec::new());
}

/// dfetch-2: the datapack file of a URL is stored without a hash to check.
#[test]
fn url_file_is_stored() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());

    let output = environment.fetch(&url);

    assert_eq!(
        fs::read(fetched_path(&output)).ok(),
        Some(example_datapack())
    );
}

/// dfetch-2: the name made from the URL is printed.
#[test]
fn name_made_from_the_url_is_printed() {
    let output = fetch_example_with_yes();

    assert!(has_line(
        &output.stdout,
        "datapack name from the URL: Example"
    ));
}

/// dfetch-2: the user is asked for the datapack name after the name made from the URL.
#[test]
fn datapack_name_is_asked() {
    let (_environment, output) = fetch_example_asking(StdinConnection::ClosedAfter(Vec::new()));

    assert!(
        output.stdout.contains(
            "mism: datapack name from the URL: Example\nmism: datapack name (empty for Example): "
        ),
        "stdout: {}",
        output.stdout
    );
}

/// dfetch-2: the answer on a terminal is the datapack name.
#[test]
fn answer_on_a_terminal_is_the_datapack_name() {
    let (_environment, output) =
        fetch_example_asking(StdinConnection::Terminal(b"Renamed\n".to_vec()));

    assert_eq!(last_record(&output.stdout)[0], "Renamed");
}

/// dfetch-2: an empty answer keeps the name made from the URL.
#[test]
fn empty_answer_keeps_the_name_made_from_the_url() {
    let (_environment, output) = fetch_example_asking(StdinConnection::Terminal(b"\n".to_vec()));

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// dfetch-2: stdin that is not a terminal and gives no input keeps the name made from the URL.
#[test]
fn no_input_keeps_the_name_made_from_the_url() {
    let (_environment, output) = fetch_example_asking(StdinConnection::ClosedAfter(Vec::new()));

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// dfetch-2: the answer is read from stdin that is not a terminal too.
#[test]
fn answer_on_a_pipe_is_the_datapack_name() {
    let (_environment, output) =
        fetch_example_asking(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()));

    assert_eq!(last_record(&output.stdout)[0], "Renamed");
}

/// dfetch-2: with `--yes` the user is not asked.
#[test]
fn yes_keeps_the_name_from_being_asked() {
    let output = fetch_example_with_yes();

    assert!(
        !output.stdout.contains("datapack name (empty for"),
        "stdout: {}",
        output.stdout
    );
}

/// dfetch-2: with `--yes` the name made from the URL is used.
#[test]
fn yes_uses_the_name_made_from_the_url() {
    let output = fetch_example_with_yes();

    assert_eq!(last_record(&output.stdout)[0], "Example");
}

/// dfetch-2: the datapack file name is the datapack name and `.zip`.
#[test]
fn file_name_is_the_datapack_name_and_zip() {
    let (_environment, output) =
        fetch_example_asking(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()));

    assert_eq!(
        fetched_path(&output).file_name().unwrap().to_str(),
        Some("Renamed.zip")
    );
}

/// dfetch-2 (「取得元の指定」 方法): the name is the last element of the URL's path without the
/// query, the fragment, and `.zip`, with its percent-encoding kept.
#[test]
fn name_from_a_url_keeps_percent_encoding_without_query_and_fragment() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar("/dl/My%20Pack.zip", &example_datapack());

    let output = environment.fetch(&format!("{url}?token=1#top"));

    assert!(has_line(
        &output.stdout,
        "datapack name from the URL: My%20Pack"
    ));
}

/// dfetch-2: only `.zip` is removed from the end of the URL's last element.
#[test]
fn other_suffix_of_the_url_stays_in_the_name() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar("/downloads/Example.jar", &example_datapack());

    let output = environment.fetch(&url);

    assert_eq!(
        fetched_path(&output).file_name().unwrap().to_str(),
        Some("Example.jar.zip")
    );
}

/// dfetch-2: `mism datapack inspect` names a datapack from a URL the same way.
#[test]
fn inspect_names_a_datapack_from_a_url_the_same_way() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());

    let output = environment
        .mism_datapack(["inspect", &url])
        .stdin(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()))
        .run();

    assert!(
        output.stdout.contains("mism: name\tRenamed\n"),
        "stdout: {}",
        output.stdout
    );
}

/// dfetch-2: `mism datapack install` names a datapack from a URL the same way.
#[test]
fn install_names_a_datapack_from_a_url_the_same_way() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());

    environment
        .mism_datapack(["install", &url])
        .stdin(StdinConnection::Terminal(b"Renamed\ny\n".to_vec()))
        .run();

    assert_eq!(
        fs::read(environment.datapacks().join("Renamed.zip")).ok(),
        Some(example_datapack())
    );
}

/// dfetch-3: the local datapack file is copied into the cache directory.
#[test]
fn local_file_is_copied() {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("Example.zip", &example_datapack());

    let output = environment.fetch(zip.to_str().unwrap());

    assert_eq!(
        fs::read(fetched_path(&output)).ok(),
        Some(example_datapack())
    );
}

/// dfetch-3: copying leaves the local file where it was.
#[test]
fn local_file_stays() {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("Example.zip", &example_datapack());

    environment.fetch(zip.to_str().unwrap());

    assert_eq!(fs::read(&zip).ok(), Some(example_datapack()));
}

/// The same file, first fetched as the local `A.zip`, then from a URL as `B`.
fn fetch_the_same_file_twice() -> (DatapackEnvironment, Output, Output) {
    let environment = DatapackEnvironment::without_server();
    let zip = environment.write_local_zip("A.zip", &example_datapack());
    let url = environment
        .modrinth
        .mount_jar("/downloads/B.zip", &example_datapack());
    let first = environment.fetch(zip.to_str().unwrap());
    let second = environment.fetch(&url);
    (environment, first, second)
}

/// dfetch-4: the message names the cached file's datapack name and path.
#[test]
fn same_file_is_reported_with_the_cached_name_and_path() {
    let (_environment, first, second) = fetch_the_same_file_twice();

    assert!(
        has_line(
            &second.stdout,
            &format!(
                "the datapack is the same as the cached datapack of A: {}",
                fetched_path(&first).display()
            )
        ),
        "stdout: {}",
        second.stdout
    );
}

/// dfetch-4: no second file is stored.
#[test]
fn same_file_is_not_stored_again() {
    let (environment, _first, _second) = fetch_the_same_file_twice();

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(
        output.stdout.lines().count(),
        1,
        "stdout: {}",
        output.stdout
    );
}

/// dfetch-4: the cached file and its datapack name are the result.
#[test]
fn same_file_keeps_the_cached_name_and_path() {
    let (_environment, first, second) = fetch_the_same_file_twice();
    let second_record = last_record(&second.stdout);

    assert_eq!(
        (second_record[0], PathBuf::from(second_record[4])),
        ("A", fetched_path(&first))
    );
}

/// dfetch-4: the fetch of the same file succeeds.
#[test]
fn fetch_of_the_same_file_succeeds() {
    let (_environment, _first, second) = fetch_the_same_file_twice();

    assert_eq!(second.exit_code, 0, "stdout: {}", second.stdout);
}

/// True Ending 1.1.4d fetched first from a URL, then from Modrinth with the same content, in a
/// server directory for `place`. Returns the URL too.
fn fetch_from_a_url_then_from_modrinth() -> (DatapackEnvironment, String) {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    let url = environment.modrinth.mount_jar(
        &format!("/downloads/{TRUE_ENDING_FILE_NAME}"),
        &true_ending_datapack(),
    );
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());
    environment.fetch(&url);
    environment.fetch(TRUE_ENDING_SOURCE);
    (environment, url)
}

/// dfetch-4, dlist-4: `mism datapack list --global` prints the later fetch's source and version
/// number with the earlier datapack name.
#[test]
fn list_shows_the_later_source_of_the_same_file() {
    let (environment, _url) = fetch_from_a_url_then_from_modrinth();

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: tru.e-ending-1.1.4d\t1.1.4d\t{}\t{TRUE_ENDING_SOURCE}\n",
            sha256_hex(&true_ending_datapack())
        )
    );
}

/// dfetch-4: `mism datapack place` finds the file by the later source.
#[test]
fn place_finds_the_same_file_by_the_later_source() {
    let (environment, _url) = fetch_from_a_url_then_from_modrinth();

    let output = environment
        .mism_datapack(["place", TRUE_ENDING_SOURCE])
        .run();

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dfetch-4: `mism datapack place` does not find the file by the earlier URL source.
#[test]
fn place_does_not_find_the_same_file_by_the_earlier_url() {
    let (environment, url) = fetch_from_a_url_then_from_modrinth();

    let output = environment.mism_datapack(["place", &url]).run();

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dfetch-4: `mism datapack place` finds the file by a local file with the same content.
#[test]
fn place_finds_the_same_file_by_a_local_file() {
    let (environment, _url) = fetch_from_a_url_then_from_modrinth();
    let zip = environment.write_local_zip(TRUE_ENDING_FILE_NAME, &true_ending_datapack());

    let output = environment
        .mism_datapack(["place", zip.to_str().unwrap()])
        .run();

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dfetch-5: a fetch from Modrinth prints the datapack name, the version number, the source,
/// and the SHA-256.
#[test]
fn modrinth_fetch_prints_its_record() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    let output = environment.fetch(TRUE_ENDING_SOURCE);

    assert_eq!(
        last_record(&output.stdout)[..4],
        [
            TRUE_ENDING_NAME,
            "1.1.4d",
            TRUE_ENDING_SOURCE,
            sha256_hex(&true_ending_datapack()).as_str()
        ]
    );
}

/// dfetch-5, dp-12: a fetch from a URL prints `-` as the version number and the URL as given.
#[test]
fn url_fetch_prints_its_record() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());

    let output = environment.fetch(&url);

    assert_eq!(
        last_record(&output.stdout)[..4],
        [
            "Example",
            "-",
            url.as_str(),
            sha256_hex(&example_datapack()).as_str()
        ]
    );
}

/// dfetch-5: a fetch of a local file prints the path as given as the source.
#[test]
fn local_fetch_prints_the_path_as_given() {
    let environment = DatapackEnvironment::without_server();
    environment.write_local_zip("Example.zip", &example_datapack());

    let output = environment.fetch("../outside/Example.zip");

    assert_eq!(last_record(&output.stdout)[2], "../outside/Example.zip");
}

/// dfetch-5: the last value is the absolute path of the file in the cache directory.
#[test]
fn printed_path_is_absolute_in_the_cache_directory() {
    let environment = DatapackEnvironment::without_server();
    environment.write_local_zip("Example.zip", &example_datapack());

    let output = environment.fetch("../outside/Example.zip");

    assert!(
        fetched_path(&output).starts_with(environment.cache()),
        "stdout: {}",
        output.stdout
    );
}

/// dfetch-5 (「キャッシュのディレクトリのプラグインとデータパック」 方法): the file is
/// `<cache directory>/datapacks/<datapack name>/<SHA-256>/<file name>`.
#[test]
fn datapack_is_stored_below_datapacks_by_name_and_sha256() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    let output = environment.fetch(TRUE_ENDING_SOURCE);

    assert_eq!(
        fetched_path(&output),
        environment
            .cache()
            .join("datapacks")
            .join(TRUE_ENDING_NAME)
            .join(sha256_hex(&true_ending_datapack()))
            .join(TRUE_ENDING_FILE_NAME)
    );
}

/// dfetch-5: a successful fetch exits with status 0.
#[test]
fn successful_fetch_exits_with_status_0() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());

    let output = environment.fetch(TRUE_ENDING_SOURCE);

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// The directories below `directory` named by 64 lowercase hexadecimal digits.
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
    found.sort();
    found
}

/// d-pcache-4
#[test]
fn same_content_as_a_plugin_and_as_a_datapack_has_a_directory_in_each() {
    let environment = DatapackEnvironment::without_server();
    let bytes = example_datapack();
    let jar = environment.write_outside("Same.jar", &bytes);
    let zip = environment.write_local_zip("Same.zip", &bytes);
    environment
        .mism(
            "plugin",
            &environment.server(),
            ["fetch", jar.to_str().unwrap()],
        )
        .run();

    environment.fetch(zip.to_str().unwrap());

    let sha256 = sha256_hex(&bytes);
    assert_eq!(
        sha256_directories(&environment.cache()),
        [
            environment.cache().join("datapacks/Same").join(&sha256),
            environment.cache().join("plugins/Same").join(&sha256),
        ]
    );
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

/// Starts fetching from a server that sends a datapack in 50 pieces 100 ms apart, and waits
/// until mism has created the cache directory, which it does right before the download.
fn start_a_slow_download(environment: &DatapackEnvironment) -> (RunningMism, SlowJarServer) {
    let server = SlowJarServer::start(&example_datapack(), 50, Duration::from_millis(100));
    let mism = environment
        .mism_datapack(["fetch", server.url(), "--yes"])
        .spawn();
    wait_for_path(&environment.cache());
    (mism, server)
}

/// Sends `SIGINT` to mism during a slow download.
fn interrupt_a_download() -> (DatapackEnvironment, Output) {
    let environment = DatapackEnvironment::without_server();
    let (mism, _server) = start_a_slow_download(&environment);
    mism.send(Signal::SIGINT);
    let output = mism.wait();
    (environment, output)
}

/// d-fetch-1: SIGINT during the download of a datapack ends mism with status 130.
#[test]
fn interrupted_download_exits_with_130() {
    let (_environment, output) = interrupt_a_download();

    assert_eq!(output.exit_code, 130);
}

/// d-fetch-1: SIGINT during the download of a datapack adds no SHA-256 directory.
#[test]
fn interrupted_download_adds_no_sha256_directory() {
    let (environment, _output) = interrupt_a_download();

    assert_eq!(
        sha256_directories(&environment.cache()),
        Vec::<PathBuf>::new()
    );
}
