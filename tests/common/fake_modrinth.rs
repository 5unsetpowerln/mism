use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const REQUEST_DEADLINE: Duration = Duration::from_secs(30);

/// The source of LuckPerms v5.5.71-bukkit, the only Modrinth version with that number, whose
/// other attributes have one kind each (plg-12).
pub const LUCKPERMS_SOURCE: &str = "modrinth:luckperms:v5.5.71-bukkit";
/// The file name of LuckPerms v5.5.71-bukkit.
pub const LUCKPERMS_FILE_NAME: &str = "LuckPerms-Bukkit-5.5.71.jar";
/// The source of the datapack of True Ending 1.1.4d, the only Modrinth version for datapacks with
/// that number, which has one file (dp-6).
pub const TRUE_ENDING_SOURCE: &str = "modrinth:true-ending:1.1.4d";
/// The file name of the datapack of True Ending 1.1.4d.
pub const TRUE_ENDING_FILE_NAME: &str = "tru.e-ending-1.1.4d.zip";
/// The name of True Ending on Modrinth (dp-8).
pub const TRUE_ENDING_NAME: &str = "True Ending - Ender Dragon Overhaul";

/// A local stand-in for the Modrinth API under `/v2`, serving the responses saved from the real
/// Modrinth in `tests/fixtures/`, served from a runtime this struct owns.
pub struct FakeModrinth {
    runtime: Runtime,
    server: MockServer,
    /// How long each response mounted from now on waits before it is sent.
    delay: Mutex<Duration>,
}

/// A file of a saved version list that the fake Modrinth serves: its URL is rewritten to point
/// at the fake Modrinth, which serves `bytes` there, and its `hashes.sha512` to
/// `published_sha512`.
pub struct ServedFile<'a> {
    pub filename: &'a str,
    pub bytes: &'a [u8],
    pub published_sha512: String,
}

impl<'a> ServedFile<'a> {
    /// Serves `bytes` as the file named `filename` with their true SHA-512.
    pub fn new(filename: &'a str, bytes: &'a [u8]) -> ServedFile<'a> {
        ServedFile {
            filename,
            bytes,
            published_sha512: sha512_hex(bytes),
        }
    }

    /// Publishes another SHA-512 than the one of the bytes served (fetch-1).
    pub fn with_published_sha512(self, published_sha512: &str) -> ServedFile<'a> {
        ServedFile {
            published_sha512: published_sha512.to_owned(),
            ..self
        }
    }
}

impl FakeModrinth {
    pub fn start() -> FakeModrinth {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(MockServer::start());
        FakeModrinth {
            runtime,
            server,
            delay: Mutex::new(Duration::ZERO),
        }
    }

    /// The value tests pass as `MISM_MODRINTH_URL`.
    pub fn base_url(&self) -> String {
        format!("{}/v2", self.server.uri())
    }

    /// Delays every response mounted after this call by `delay` (d-modrinth-1).
    pub fn delay_responses(&self, delay: Duration) {
        *self.delay.lock().unwrap() = delay;
    }

    /// Serves the saved `modrinth-search-<fixture_name>.json` for the search term `query` among
    /// the projects of `project_type` (`plugin` or `datapack`).
    pub fn mount_search(&self, query: &str, project_type: &str, fixture_name: &str) {
        self.mount_search_json(query, project_type, &saved_search(fixture_name));
    }

    /// Serves `results`, a search response that the test may have changed, for the search term
    /// `query` among the projects of `project_type`.
    pub fn mount_search_json(&self, query: &str, project_type: &str, results: &Value) {
        let mock = Mock::given(method("GET"))
            .and(path("/v2/search"))
            .and(query_param("query", query))
            .and(query_param(
                "facets",
                format!(r#"[["project_type:{project_type}"]]"#),
            ));
        self.mount(mock, self.json(&results.to_string()));
    }

    /// Serves the saved `modrinth-project-<slug>.json` at `/v2/project/<slug>`.
    pub fn mount_project(&self, slug: &str) {
        self.mount_project_json(slug, &saved_project(slug));
    }

    /// Serves `project`, a project response that the test may have changed, at
    /// `/v2/project/<identifier>`.
    pub fn mount_project_json(&self, identifier: &str, project: &Value) {
        self.mount_json_at(&format!("/v2/project/{identifier}"), &project.to_string());
    }

    /// Serves the saved version list of `slug` at its endpoint, with the files in `served`
    /// rewritten and served (see `ServedFile`).
    pub fn mount_version_list(&self, slug: &str, served: &[ServedFile]) {
        let mut versions = saved_version_list(slug);
        self.serve_files(&mut versions, served);
        self.mount_version_list_json(slug, &versions);
    }

    /// Serves each of `served` and points its file in `versions`, a version list, at it (see
    /// `ServedFile`).
    pub fn serve_files(&self, versions: &mut Value, served: &[ServedFile]) {
        for file in served {
            let url = self.mount_file(file.filename, file.bytes);
            rewrite_file(versions, file.filename, &url, &file.published_sha512);
        }
    }

    /// Serves LuckPerms' saved project and version list, with `bytes` as the file of
    /// v5.5.71-bukkit.
    pub fn serve_luckperms(&self, bytes: &[u8]) {
        self.mount_project("luckperms");
        self.mount_version_list("luckperms", &[ServedFile::new(LUCKPERMS_FILE_NAME, bytes)]);
    }

    /// Serves True Ending's saved project and version list, with `bytes` as the file of the
    /// datapack 1.1.4d.
    pub fn serve_true_ending(&self, bytes: &[u8]) {
        self.mount_project("true-ending");
        self.mount_version_list(
            "true-ending",
            &[ServedFile::new(TRUE_ENDING_FILE_NAME, bytes)],
        );
    }

    /// Serves `versions`, a version list that the test changed, at the endpoint of `slug`.
    pub fn mount_version_list_json(&self, slug: &str, versions: &Value) {
        let mock = Mock::given(method("GET"))
            .and(path(format!("/v2/project/{}/version", project_id(slug))))
            .and(query_param("include_changelog", "false"));
        self.mount(mock, self.json(&versions.to_string()));
    }

    /// Serves the saved `modrinth-projects-<fixture_name>.json` for the project ids `ids`, which
    /// mism requests as a JSON array in this order.
    pub fn mount_projects(&self, fixture_name: &str, ids: &[&str]) {
        let body = fixture(&format!("modrinth-projects-{fixture_name}.json"));
        let mock = Mock::given(method("GET"))
            .and(path("/v2/projects"))
            .and(query_param("ids", serde_json::to_string(ids).unwrap()));
        self.mount(mock, self.json(&body));
    }

    /// Answers requests for `url_path` with `status` and `body` (e.g. 429, 404, or 500).
    pub fn mount_status(&self, url_path: &str, status: u16, body: &str) {
        let mock = Mock::given(method("GET")).and(path(url_path));
        self.mount(
            mock,
            ResponseTemplate::new(status)
                .set_body_raw(body.as_bytes().to_vec(), "application/json"),
        );
    }

    /// Serves `bytes` at `url_path` (e.g. `/downloads/Example.jar`) for a test that fetches from
    /// a URL (fetch-2), and returns the URL.
    pub fn mount_jar(&self, url_path: &str, bytes: &[u8]) -> String {
        let mock = Mock::given(method("GET")).and(path(url_path));
        self.mount(mock, self.response().set_body_bytes(bytes));
        format!("{}{url_path}", self.server.uri())
    }

    /// Serves `bytes` at `url_path` for the first request only, and returns the URL. A mock
    /// mounted later at the same path answers the later requests.
    pub fn mount_jar_once(&self, url_path: &str, bytes: &[u8]) -> String {
        let mock = Mock::given(method("GET"))
            .and(path(url_path))
            .respond_with(self.response().set_body_bytes(bytes))
            .up_to_n_times(1);
        self.runtime.block_on(mock.mount(&self.server));
        format!("{}{url_path}", self.server.uri())
    }

    /// The requests received so far, in the order they arrived.
    pub fn requests(&self) -> Vec<Request> {
        self.runtime
            .block_on(self.server.received_requests())
            .expect("request recording is enabled by default")
    }

    /// The URL paths of the requests received so far, in the order they arrived.
    pub fn request_paths(&self) -> Vec<String> {
        self.requests()
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect()
    }

    /// The value of the query parameter `name` in the first request for `url_path`.
    pub fn query_parameter(&self, url_path: &str, name: &str) -> Option<String> {
        self.requests()
            .iter()
            .find(|request| request.url.path() == url_path)?
            .url
            .query_pairs()
            .find(|(parameter, _)| parameter == name)
            .map(|(_, value)| value.into_owned())
    }

    pub fn request_count(&self) -> usize {
        self.requests().len()
    }

    /// Waits until `count` requests have arrived, failing the test after 30 seconds.
    pub fn wait_for_requests(&self, count: usize) {
        let deadline = Instant::now() + REQUEST_DEADLINE;
        while self.request_count() < count {
            assert!(
                Instant::now() < deadline,
                "{count} requests did not arrive within {REQUEST_DEADLINE:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Serves a file at `/files/<file name>` and returns its URL.
    fn mount_file(&self, filename: &str, bytes: &[u8]) -> String {
        self.mount_jar(&format!("/files/{filename}"), bytes)
    }

    fn mount_json_at(&self, url_path: &str, body: &str) {
        self.mount(
            Mock::given(method("GET")).and(path(url_path)),
            self.json(body),
        );
    }

    fn response(&self) -> ResponseTemplate {
        ResponseTemplate::new(200).set_delay(*self.delay.lock().unwrap())
    }

    fn json(&self, body: &str) -> ResponseTemplate {
        self.response()
            .set_body_raw(body.as_bytes().to_vec(), "application/json")
    }

    fn mount(&self, mock: wiremock::MockBuilder, response: ResponseTemplate) {
        self.runtime
            .block_on(mock.respond_with(response).mount(&self.server));
    }
}

/// The saved version list of `slug`, for tests that change it before serving it with
/// `mount_version_list_json`.
pub fn saved_version_list(slug: &str) -> Value {
    serde_json::from_str(&fixture(&format!("modrinth-versions-{slug}.json"))).unwrap()
}

/// The saved `modrinth-search-<fixture_name>.json`, for tests that change it before serving it
/// with `mount_search_json`.
pub fn saved_search(fixture_name: &str) -> Value {
    serde_json::from_str(&fixture(&format!("modrinth-search-{fixture_name}.json"))).unwrap()
}

/// The saved project response of `slug`, for tests that change it before serving it with
/// `mount_project_json`.
pub fn saved_project(slug: &str) -> Value {
    serde_json::from_str(&fixture(&format!("modrinth-project-{slug}.json"))).unwrap()
}

/// The project id of `slug`, from its saved project response or a saved search response.
pub fn project_id(slug: &str) -> String {
    let project_fixture = format!("modrinth-project-{slug}.json");
    if fixture_path(&project_fixture).exists() {
        let project: Value = serde_json::from_str(&fixture(&project_fixture)).unwrap();
        return project["id"].as_str().unwrap().to_owned();
    }
    saved_search_hits()
        .into_iter()
        .find(|hit| hit["slug"] == slug)
        .unwrap_or_else(|| panic!("no saved response has the project id of {slug}"))["project_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The hits of every saved `modrinth-search-*.json`.
fn saved_search_hits() -> Vec<Value> {
    let mut hits = Vec::new();
    for entry in fs::read_dir(fixture_path("")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.starts_with("modrinth-search-") {
            let search: Value = serde_json::from_str(&fixture(&name)).unwrap();
            hits.extend(search["hits"].as_array().unwrap().iter().cloned());
        }
    }
    hits
}

/// Points every file named `filename` in `versions` at `url` with `sha512`.
fn rewrite_file(versions: &mut Value, filename: &str, url: &str, sha512: &str) {
    let mut rewritten = 0;
    for version in versions.as_array_mut().unwrap() {
        for file in version["files"].as_array_mut().unwrap() {
            if file["filename"] == filename {
                file["url"] = url.into();
                file["hashes"]["sha512"] = sha512.into();
                rewritten += 1;
            }
        }
    }
    assert!(
        rewritten > 0,
        "the saved version list has no file {filename}"
    );
}

/// The SHA-512 of `bytes` in lowercase hexadecimal, as Modrinth publishes it.
pub fn sha512_hex(bytes: &[u8]) -> String {
    hex(&Sha512::digest(bytes))
}

/// The SHA-256 of `bytes` in lowercase hexadecimal, as mism prints and records it.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).unwrap()
}
