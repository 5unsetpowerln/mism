use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A local stand-in for the PaperMC API, served from a runtime this struct owns.
pub struct FakePaperMc {
    runtime: Runtime,
    server: MockServer,
}

impl FakePaperMc {
    pub fn start() -> FakePaperMc {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(MockServer::start());
        FakePaperMc { runtime, server }
    }

    /// The value tests pass as `MISM_PAPERMC_URL`.
    pub fn base_url(&self) -> String {
        format!("{}/v3", self.server.uri())
    }

    pub fn mount_version_list(&self, project: &str, body: &str) {
        self.mount_json(&format!("/v3/projects/{project}/versions"), body);
    }

    pub fn mount_version_list_from_fixture(&self, project: &str) {
        self.mount_version_list(project, &fixture(&format!("{project}-versions.json")));
    }

    pub fn mount_download(
        &self,
        project: &str,
        minecraft_version: &str,
        software_version: u64,
        body: &str,
    ) {
        self.mount_json(
            &format!(
                "/v3/projects/{project}/versions/{minecraft_version}/builds/{software_version}"
            ),
            body,
        );
    }

    /// Serves `jar_bytes` as the jar `<project>-<minecraft_version>-<software_version>.jar`, and
    /// at the download endpoint of that version its download information (see
    /// `mount_download_information`). Returns the jar's URL.
    pub fn mount_jar_download(
        &self,
        project: &str,
        minecraft_version: &str,
        software_version: u64,
        jar_bytes: &[u8],
        published_sha256: &str,
    ) -> String {
        self.mount_jar_download_after(
            project,
            minecraft_version,
            software_version,
            jar_bytes,
            published_sha256,
            Duration::ZERO,
        )
    }

    /// Like `mount_jar_download`, but the jar is served `delay` after its request arrives.
    pub fn mount_jar_download_after(
        &self,
        project: &str,
        minecraft_version: &str,
        software_version: u64,
        jar_bytes: &[u8],
        published_sha256: &str,
        delay: Duration,
    ) -> String {
        let jar_path = format!(
            "/jars/{}",
            jar_name(project, minecraft_version, software_version)
        );
        let jar_url = format!("{}{jar_path}", self.server.uri());
        self.mount(
            &jar_path,
            ResponseTemplate::new(200)
                .set_body_bytes(jar_bytes)
                .set_delay(delay),
        );
        self.mount_download_information(
            project,
            minecraft_version,
            software_version,
            jar_bytes,
            published_sha256,
            &jar_url,
        );
        jar_url
    }

    /// Serves at the download endpoint of `<project> <minecraft_version> <software_version>` the
    /// saved download information of the project, with the name replaced by
    /// `<project>-<minecraft_version>-<software_version>.jar`, the size by that of `jar_bytes`,
    /// the SHA-256 by `published_sha256`, and the URL by `jar_url`.
    pub fn mount_download_information(
        &self,
        project: &str,
        minecraft_version: &str,
        software_version: u64,
        jar_bytes: &[u8],
        published_sha256: &str,
        jar_url: &str,
    ) {
        let mut information = saved_download_information(project);
        information["id"] = software_version.into();
        let artifact = &mut information["downloads"]["server:default"];
        artifact["name"] = jar_name(project, minecraft_version, software_version).into();
        artifact["checksums"]["sha256"] = published_sha256.into();
        artifact["size"] = jar_bytes.len().into();
        artifact["url"] = jar_url.into();
        self.mount_download(
            project,
            minecraft_version,
            software_version,
            &information.to_string(),
        );
    }

    pub fn request_count(&self) -> usize {
        self.request_paths().len()
    }

    /// The number of requests received for the URL path of `url`.
    pub fn request_count_for(&self, url: &str) -> usize {
        let url_path = url.strip_prefix(&self.server.uri()).unwrap();
        self.request_paths()
            .iter()
            .filter(|path| path == &url_path)
            .count()
    }

    /// The URL paths of the requests received so far, in the order they arrived.
    pub fn request_paths(&self) -> Vec<String> {
        self.runtime
            .block_on(self.server.received_requests())
            .expect("request recording is enabled by default")
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect()
    }

    fn mount_json(&self, url_path: &str, body: &str) {
        self.mount(
            url_path,
            ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "application/json"),
        );
    }

    fn mount(&self, url_path: &str, response: ResponseTemplate) {
        let mock = Mock::given(method("GET"))
            .and(path(url_path))
            .respond_with(response);
        self.runtime.block_on(mock.mount(&self.server));
    }
}

/// The saved version list of `project` (`paper` or `folia`), for tests that change it before
/// serving it with `mount_version_list`.
pub fn saved_version_list(project: &str) -> Value {
    serde_json::from_str(&fixture(&format!("{project}-versions.json"))).unwrap()
}

/// A base URL on which nothing listens.
pub fn unreachable_base_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/v3")
}

/// The SHA-256 of `bytes` in lowercase hexadecimal, as PaperMC publishes it.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The jar name of `<project> <minecraft_version> <software_version>` in PaperMC's form (e.g.
/// `paper-26.3-206.jar`).
fn jar_name(project: &str, minecraft_version: &str, software_version: u64) -> String {
    format!("{project}-{minecraft_version}-{software_version}.jar")
}

fn saved_download_information(project: &str) -> Value {
    let name = match project {
        "paper" => "paper-26.3-206.json",
        "folia" => "folia-26.3-3.json",
        _ => panic!("no download information of {project} is saved"),
    };
    serde_json::from_str(&fixture(name)).unwrap()
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(path).unwrap()
}
