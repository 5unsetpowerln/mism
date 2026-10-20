use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::atomic_fs::{self, UnnamedFile};
use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("neither XDG_CACHE_HOME nor HOME is set, so the cache directory cannot be determined")]
    HomeNotSet,
    #[error("failed to create the directory {}", path.display())]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read the cached jar {}", path.display())]
    ReadCached {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to remove the cached jar {} whose SHA-256 is wrong", path.display())]
    RemoveStale {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the SHA-256 of {url} is {actual}, but PaperMC publishes {expected}")]
    Sha256Mismatch {
        url: String,
        expected: String,
        actual: String,
    },
    #[error(transparent)]
    Download(#[from] http_client::Error),
    #[error(transparent)]
    AtomicFs(#[from] atomic_fs::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Sha256Mismatch { .. } => Some(
                "run the command again; if this repeats, check the download URL in a browser"
                    .to_owned(),
            ),
            Error::Download(error) => error.hint(),
            Error::AtomicFs(error) => error.hint(),
            Error::HomeNotSet
            | Error::CreateDirectory { .. }
            | Error::ReadCached { .. }
            | Error::RemoveStale { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Download(error) => error.interrupted_by(),
            Error::HomeNotSet
            | Error::CreateDirectory { .. }
            | Error::ReadCached { .. }
            | Error::RemoveStale { .. }
            | Error::Sha256Mismatch { .. }
            | Error::AtomicFs(_) => None,
        }
    }
}

/// Returns the path of the jar in the cache directory, downloading it first unless a jar with
/// the published SHA-256 is already there.
pub fn obtain(
    client: &HttpClient,
    project_name: &str,
    minecraft_version: &str,
    software_version: u64,
    jar_name: &str,
    sha256: &str,
    url: &str,
) -> Result<PathBuf, Error> {
    let jar_path = jar_path(
        &cache_directory()?,
        project_name,
        minecraft_version,
        software_version,
        jar_name,
    );
    obtain_at(client, &jar_path, sha256, url)?;
    Ok(jar_path)
}

fn cache_directory() -> Result<PathBuf, Error> {
    cache_directory_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
}

fn cache_directory_from(
    xdg_cache_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, Error> {
    match xdg_cache_home.filter(|value| !value.is_empty()) {
        Some(xdg_cache_home) => Ok(PathBuf::from(xdg_cache_home).join("mism")),
        None => Ok(PathBuf::from(home.ok_or(Error::HomeNotSet)?)
            .join(".cache")
            .join("mism")),
    }
}

fn jar_path(
    cache_directory: &Path,
    project_name: &str,
    minecraft_version: &str,
    software_version: u64,
    jar_name: &str,
) -> PathBuf {
    cache_directory
        .join(project_name)
        .join(minecraft_version)
        .join(software_version.to_string())
        .join(jar_name)
}

fn obtain_at(
    client: &HttpClient,
    jar_path: &Path,
    expected_sha256: &str,
    url: &str,
) -> Result<(), Error> {
    let directory = jar_path
        .parent()
        .expect("a jar path has a parent directory");
    fs::create_dir_all(directory).map_err(|source| Error::CreateDirectory {
        path: directory.to_path_buf(),
        source,
    })?;
    // Step 1.
    let existing_sha256 = sha256_of_existing_file(jar_path)?;
    if existing_sha256.as_deref() == Some(expected_sha256) {
        return Ok(());
    }
    // Steps 2 to 4.
    let downloaded = download_verified(client, directory, expected_sha256, url)?;
    // Step 5.
    if existing_sha256.is_some() {
        remove_stale_file(jar_path)?;
    }
    match downloaded.link_as(jar_path) {
        Ok(()) => Ok(()),
        // Step 6: another mism named its file first.
        Err(atomic_fs::Error::AlreadyExists { .. }) => {
            verify_existing_file(jar_path, expected_sha256, url)
        }
        Err(error) => Err(error.into()),
    }
}

/// The lowercase hex SHA-256 of the file, or `None` when there is no such file.
fn sha256_of_existing_file(path: &Path) -> Result<Option<String>, Error> {
    let read_failed = |source| Error::ReadCached {
        path: path.to_path_buf(),
        source,
    };
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(read_failed(error)),
    };
    let mut hasher = Sha256::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        match file.read(&mut chunk).map_err(read_failed)? {
            0 => break,
            length => hasher.update(&chunk[..length]),
        }
    }
    Ok(Some(to_hex(&hasher.finalize())))
}

fn verify_existing_file(path: &Path, expected_sha256: &str, url: &str) -> Result<(), Error> {
    let actual = sha256_of_existing_file(path)?.unwrap_or_default();
    if actual == expected_sha256 {
        Ok(())
    } else {
        Err(Error::Sha256Mismatch {
            url: url.to_owned(),
            expected: expected_sha256.to_owned(),
            actual,
        })
    }
}

/// Downloads `url` into an unnamed file in `directory`; fails if its SHA-256 is not the expected one.
fn download_verified(
    client: &HttpClient,
    directory: &Path,
    expected_sha256: &str,
    url: &str,
) -> Result<UnnamedFile, Error> {
    let mut sink = HashingWriter {
        file: UnnamedFile::create_in(directory)?,
        hasher: Sha256::new(),
    };
    client.download(url, &mut sink)?;
    let actual = to_hex(&sink.hasher.finalize());
    if actual == expected_sha256 {
        Ok(sink.file)
    } else {
        Err(Error::Sha256Mismatch {
            url: url.to_owned(),
            expected: expected_sha256.to_owned(),
            actual,
        })
    }
}

fn remove_stale_file(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        // Another mism may have replaced the stale file already.
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result.map_err(|source| Error::RemoveStale {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn to_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes to `file` and hashes the same bytes.
struct HashingWriter {
    file: UnnamedFile,
    hasher: Sha256,
}

impl Write for HashingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.file.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::thread;

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use tokio::runtime::Runtime;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{Error, cache_directory_from, jar_path, obtain_at};
    use crate::http_client::HttpClient;

    const JAR_BYTES: &[u8] = b"pretend this is a jar";

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        super::to_hex(&Sha256::digest(bytes))
    }

    /// A fake PaperMC download server that must stay alive with its runtime.
    struct FakeDownload {
        runtime: Runtime,
        server: MockServer,
    }

    impl FakeDownload {
        fn serving(body: &[u8]) -> FakeDownload {
            let runtime = Runtime::new().unwrap();
            let server = runtime.block_on(async {
                let server = MockServer::start().await;
                Mock::given(method("GET"))
                    .and(path("/paper.jar"))
                    .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
                    .mount(&server)
                    .await;
                server
            });
            FakeDownload { runtime, server }
        }

        fn url(&self) -> String {
            format!("{}/paper.jar", self.server.uri())
        }

        fn request_count(&self) -> usize {
            self.runtime
                .block_on(self.server.received_requests())
                .unwrap()
                .len()
        }
    }

    fn jar_in(cache: &Path) -> PathBuf {
        jar_path(cache, "paper", "26.3", 206, "paper-26.3-206.jar")
    }

    fn obtain_jar(cache: &Path, download: &FakeDownload, sha256: &str) -> Result<(), Error> {
        let client = HttpClient::new().unwrap();
        obtain_at(&client, &jar_in(cache), sha256, &download.url())
    }

    fn put_jar(cache: &Path, content: &[u8]) {
        let path = jar_in(cache);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// Maps each file under `directory`, relative to it, to its content.
    fn files_under(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = BTreeMap::new();
        collect_files(directory, directory, &mut files);
        files
    }

    fn collect_files(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                collect_files(root, &entry.path(), files);
            } else {
                let relative = entry.path().strip_prefix(root).unwrap().to_path_buf();
                files.insert(relative, fs::read(entry.path()).unwrap());
            }
        }
    }

    /// d-cache-2: a downloaded jar is cached under the jar path with the content served.
    #[test]
    fn fresh_download_is_cached_with_the_served_content() {
        let cache = temporary_directory();
        let download = FakeDownload::serving(JAR_BYTES);

        obtain_jar(cache.path(), &download, &sha256_hex(JAR_BYTES)).unwrap();

        assert_eq!(fs::read(jar_in(cache.path())).unwrap(), JAR_BYTES);
    }

    /// d-cache-2: the cached jar has the published SHA-256.
    #[test]
    fn fresh_download_is_cached_with_the_published_hash() {
        let cache = temporary_directory();
        let download = FakeDownload::serving(JAR_BYTES);
        let expected = sha256_hex(JAR_BYTES);

        obtain_jar(cache.path(), &download, &expected).unwrap();

        assert_eq!(
            sha256_hex(&fs::read(jar_in(cache.path())).unwrap()),
            expected
        );
    }

    /// cache-3: a cached jar with the published SHA-256 is not downloaded again.
    #[test]
    fn matching_cached_jar_causes_no_request() {
        let cache = temporary_directory();
        put_jar(cache.path(), JAR_BYTES);
        let download = FakeDownload::serving(JAR_BYTES);

        obtain_jar(cache.path(), &download, &sha256_hex(JAR_BYTES)).unwrap();

        assert_eq!(download.request_count(), 0);
    }

    /// cache-4: a cached jar with another SHA-256 is replaced by the downloaded one.
    #[test]
    fn mismatching_cached_jar_is_replaced() {
        let cache = temporary_directory();
        put_jar(cache.path(), b"corrupted");
        let download = FakeDownload::serving(JAR_BYTES);

        obtain_jar(cache.path(), &download, &sha256_hex(JAR_BYTES)).unwrap();

        assert_eq!(fs::read(jar_in(cache.path())).unwrap(), JAR_BYTES);
    }

    /// err-6: a download whose SHA-256 differs from the published one is an error.
    #[test]
    fn downloaded_jar_with_wrong_hash_is_an_error() {
        let cache = temporary_directory();
        let download = FakeDownload::serving(JAR_BYTES);

        let result = obtain_jar(cache.path(), &download, &sha256_hex(b"something else"));

        assert!(matches!(result, Err(Error::Sha256Mismatch { .. })));
    }

    /// d-cache-1: a rejected download leaves no named file in the cache directory.
    #[test]
    fn rejected_download_leaves_no_file() {
        let cache = temporary_directory();
        let download = FakeDownload::serving(JAR_BYTES);

        let _ = obtain_jar(cache.path(), &download, &sha256_hex(b"something else"));

        assert_eq!(files_under(cache.path()), BTreeMap::new());
    }

    /// d-cache-3: two simultaneous downloads of one jar both succeed.
    #[test]
    fn simultaneous_downloads_both_succeed() {
        let cache = temporary_directory();
        let download = FakeDownload::serving(JAR_BYTES);
        let expected = sha256_hex(JAR_BYTES);

        let results: Vec<_> = thread::scope(|scope| {
            let handles: Vec<_> = (0..2)
                .map(|_| scope.spawn(|| obtain_jar(cache.path(), &download, &expected)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        });

        assert!(results.iter().all(Result::is_ok));
    }

    /// d-cache-3: after two simultaneous downloads of one jar, the directory has exactly one file.
    #[test]
    fn simultaneous_downloads_leave_one_file() {
        let cache = temporary_directory();
        let download = FakeDownload::serving(JAR_BYTES);
        let expected = sha256_hex(JAR_BYTES);

        thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| obtain_jar(cache.path(), &download, &expected));
            }
        });

        assert_eq!(
            files_under(cache.path()),
            BTreeMap::from([(jar_in(Path::new("")), JAR_BYTES.to_vec())])
        );
    }

    /// cache-6: jars of other versions are untouched.
    #[test]
    fn jars_of_other_versions_are_untouched() {
        let cache = temporary_directory();
        let other = jar_path(cache.path(), "paper", "26.2", 100, "paper-26.2-100.jar");
        fs::create_dir_all(other.parent().unwrap()).unwrap();
        fs::write(&other, b"older jar").unwrap();
        let download = FakeDownload::serving(JAR_BYTES);

        obtain_jar(cache.path(), &download, &sha256_hex(JAR_BYTES)).unwrap();

        assert_eq!(fs::read(other).unwrap(), b"older jar");
    }

    /// cache-1: XDG_CACHE_HOME selects the cache directory.
    #[test]
    fn xdg_cache_home_selects_the_cache_directory() {
        let directory = cache_directory_from(Some("/xdg".into()), Some("/home/user".into()));

        assert_eq!(directory.unwrap(), Path::new("/xdg/mism"));
    }

    /// cache-2: without XDG_CACHE_HOME the cache directory is under HOME.
    #[test]
    fn home_cache_is_used_without_xdg_cache_home() {
        let directory = cache_directory_from(None, Some("/home/user".into()));

        assert_eq!(directory.unwrap(), Path::new("/home/user/.cache/mism"));
    }

    /// cache-2: an empty XDG_CACHE_HOME counts as unset.
    #[test]
    fn empty_xdg_cache_home_counts_as_unset() {
        let directory = cache_directory_from(Some(OsString::new()), Some("/home/user".into()));

        assert_eq!(directory.unwrap(), Path::new("/home/user/.cache/mism"));
    }

    /// cache-2: with neither variable set there is no fallback.
    #[test]
    fn missing_home_is_an_error() {
        assert!(matches!(
            cache_directory_from(None, None),
            Err(Error::HomeNotSet)
        ));
    }
}
