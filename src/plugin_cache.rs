use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_fs::{self, UnnamedFile};
use crate::cache_directory;
use crate::content_hash::{self, HashingWriter};
use crate::file_name;
use crate::http_client::{self, HttpClient};
use crate::interruption::{self, Signal};

const PLUGINS_DIRECTORY_NAME: &str = "plugins";
const SOURCE_TOML_FILE_NAME: &str = "source.toml";
const SHA256_HEX_LENGTH: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    CacheDirectory(#[from] cache_directory::Error),
    #[error(transparent)]
    FileName(#[from] file_name::Error),
    #[error("failed to create the directory {}", path.display())]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to list the directory {}", path.display())]
    ListDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read {}", path.display())]
    ReadSourceToml {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a valid source.toml", path.display())]
    ParseSourceToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("the file name recorded in {} cannot be used", path.display())]
    RecordedFileName {
        path: PathBuf,
        #[source]
        source: file_name::Error,
    },
    #[error("failed to write {}", path.display())]
    WriteSourceToml {
        path: PathBuf,
        #[source]
        source: atomic_fs::Error,
    },
    #[error("failed to copy {} into the cache directory", path.display())]
    CopyLocalJar {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    HashLocalJar(content_hash::Error),
    #[error(transparent)]
    Download(#[from] http_client::Error),
    #[error("the SHA-512 of {url} is {actual}, but Modrinth publishes {expected}")]
    Sha512Mismatch {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("interrupted by {0}")]
    Interrupted(Signal),
    #[error("several directories of the cache directory hold the jar with the SHA-256 {sha256}: {}", list_paths(.paths))]
    DuplicateSha256 { sha256: String, paths: Vec<PathBuf> },
    #[error("the cache directory has several jars fetched from {fetched_from}: {}", list_paths(.paths))]
    SeveralJars {
        fetched_from: String,
        paths: Vec<PathBuf>,
    },
    #[error(transparent)]
    AtomicFs(#[from] atomic_fs::Error),
    #[error("failed to remove the working directory {}", path.display())]
    RemoveWorkingDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to remove the working directory {} ({removal}) after this failure", path.display())]
    LeftWorkingDirectory {
        path: PathBuf,
        removal: io::Error,
        #[source]
        failure: Box<Error>,
    },
}

fn list_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::ReadSourceToml { path, .. }
            | Error::ParseSourceToml { path, .. }
            | Error::RecordedFileName { path, .. } => Some(format!(
                "remove the directory {} and fetch the plugin again",
                path.parent()
                    .expect("a source.toml is in a SHA-256 directory")
                    .display()
            )),
            Error::Sha512Mismatch { .. } => Some(
                "run the command again; if this repeats, check the file on Modrinth".to_owned(),
            ),
            Error::SeveralJars { .. } => Some(
                "give the path of one of these jars in the cache directory as the source"
                    .to_owned(),
            ),
            Error::CacheDirectory(error) => error.hint(),
            Error::FileName(error) => error.hint(),
            Error::HashLocalJar(error) => error.hint(),
            Error::Download(error) => error.hint(),
            Error::AtomicFs(error) => error.hint(),
            Error::LeftWorkingDirectory { failure, .. } => failure.hint(),
            Error::CreateDirectory { .. }
            | Error::ListDirectory { .. }
            | Error::WriteSourceToml { .. }
            | Error::CopyLocalJar { .. }
            | Error::Interrupted(_)
            | Error::DuplicateSha256 { .. }
            | Error::RemoveWorkingDirectory { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interrupted(signal) => Some(*signal),
            Error::Download(error) => error.interrupted_by(),
            Error::LeftWorkingDirectory { failure, .. } => failure.interrupted_by(),
            _ => None,
        }
    }
}

/// The `source.toml` of a SHA-256 directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceToml {
    pub name: String,
    pub source: String,
    pub file_name: String,
    /// Only when the jar was fetched from Modrinth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modrinth: Option<ModrinthRecord>,
}

/// What Modrinth states about the Modrinth version the fetched file belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModrinthRecord {
    /// In Modrinth's order.
    pub loaders: Vec<String>,
    /// In Modrinth's order.
    pub game_versions: Vec<String>,
    /// The identifiers, in ascending order.
    pub required_dependencies: Vec<String>,
}

/// A jar in the cache directory, as its `source.toml` records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedJar {
    pub name: String,
    pub source: String,
    pub file_name: String,
    /// The name of its SHA-256 directory.
    pub sha256: String,
    /// The absolute path of the jar.
    pub path: PathBuf,
    pub modrinth: Option<ModrinthRecord>,
}

/// Where the content of a jar to store comes from.
#[derive(Debug, Clone, Copy)]
pub enum Origin<'a> {
    /// A Modrinth file, checked against the SHA-512 Modrinth publishes (fetch-1).
    Modrinth { url: &'a str, sha512: &'a str },
    /// A URL given by the user (fetch-2).
    Url(&'a str),
    /// A local jar file (fetch-3).
    File(&'a Path),
}

/// A jar to store and what to record about it.
#[derive(Debug, Clone)]
pub struct Fetched<'a> {
    pub origin: Origin<'a>,
    pub plugin_name: &'a str,
    pub file_name: &'a str,
    /// The source as `source.toml` records it.
    pub source: &'a str,
    pub modrinth: Option<ModrinthRecord>,
}

/// The result of storing a jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// A new SHA-256 directory holds the jar.
    New(CachedJar),
    /// The cache directory already holds the same jar (fetch-4).
    AlreadyCached(AlreadyCached),
}

/// The jar already in the cache directory with the content just fetched, whose `source.toml`
/// does not record this fetch yet. The caller writes the fetch-4 message with its name and path,
/// and then calls `record_fetch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlreadyCached {
    /// The jar as its `source.toml` records it before this fetch.
    pub cached_jar: CachedJar,
    source: String,
    modrinth: Option<ModrinthRecord>,
}

impl AlreadyCached {
    /// The rest of step 6: its `source.toml` records this fetch's source and Modrinth's
    /// statements, and keeps its name and file name. Returns the jar as now recorded.
    pub fn record_fetch(self) -> Result<CachedJar, Error> {
        let directory = self
            .cached_jar
            .path
            .parent()
            .expect("a cached jar is in its SHA-256 directory");
        let source_toml = SourceToml {
            name: self.cached_jar.name.clone(),
            source: self.source,
            file_name: self.cached_jar.file_name.clone(),
            modrinth: self.modrinth,
        };
        write_source_toml(directory, &source_toml)?;
        Ok(CachedJar {
            source: source_toml.source,
            modrinth: source_toml.modrinth,
            ..self.cached_jar
        })
    }
}

/// Stores the jar in the cache directory (fetch-1 to fetch-4), after checking its names. The
/// unnamed file of a jar already cached is closed here.
pub fn store(client: &HttpClient, fetched: &Fetched) -> Result<Stored, Error> {
    store_in(&cache_directory::path()?, client, fetched)
}

/// The jar fetched with exactly `source` (a Modrinth or URL source) (place-1, insp-1). `None`
/// when there is none.
pub fn find_by_source(source: &str) -> Result<Option<CachedJar>, Error> {
    find_by_source_in(&cache_directory::path()?, source)
}

/// The jar with the content of the local jar file at `path`, taken as fetched with `source`
/// without Modrinth's statements (place-1, insp-1). `None` when there is none.
pub fn find_by_content(path: &Path, source: &str) -> Result<Option<CachedJar>, Error> {
    find_by_content_in(&cache_directory::path()?, path, source)
}

/// Every jar in the cache directory, by name and then by SHA-256 (list-3).
pub fn list() -> Result<Vec<CachedJar>, Error> {
    list_in(&cache_directory::path()?)
}

fn store_in(cache: &Path, client: &HttpClient, fetched: &Fetched) -> Result<Stored, Error> {
    file_name::check_plugin_name(fetched.plugin_name)?;
    file_name::check_jar_file_name(fetched.file_name)?;
    let (file, sha256) = receive_verified(cache, client, fetched.origin)?;
    fail_if_interrupted()?;
    match find_sha256_directory(cache, &sha256)? {
        Some(directory) => already_cached(&directory, fetched),
        None => store_new(cache, file, &sha256, fetched),
    }
}

/// Steps 1 to 3: the content in an unnamed file in the cache directory, and its SHA-256.
fn receive_verified(
    cache: &Path,
    client: &HttpClient,
    origin: Origin,
) -> Result<(UnnamedFile, String), Error> {
    create_directory(cache)?;
    let file = UnnamedFile::create_in(cache)?;
    let hashed = match origin {
        Origin::Modrinth { url, .. } => {
            let mut hashing = HashingWriter::with_sha512(file);
            client.download(url, &mut hashing)?;
            hashing.finish()
        }
        Origin::Url(url) => {
            let mut hashing = HashingWriter::new(file);
            client.download(url, &mut hashing)?;
            hashing.finish()
        }
        Origin::File(path) => copy_local_jar(path, file)?,
    };
    if let Origin::Modrinth { url, sha512 } = origin {
        verify_sha512(url, sha512, hashed.sha512.as_deref())?;
    }
    Ok((hashed.inner, hashed.sha256))
}

fn copy_local_jar(
    path: &Path,
    file: UnnamedFile,
) -> Result<content_hash::Hashed<UnnamedFile>, Error> {
    let copy_failed = |source| Error::CopyLocalJar {
        path: path.to_path_buf(),
        source,
    };
    let mut hashing = HashingWriter::new(file);
    io::copy(&mut File::open(path).map_err(copy_failed)?, &mut hashing).map_err(copy_failed)?;
    Ok(hashing.finish())
}

/// Step 3 (fetch-1).
fn verify_sha512(url: &str, expected: &str, actual: Option<&str>) -> Result<(), Error> {
    let actual = actual.expect("a Modrinth download is hashed with SHA-512");
    if actual == expected {
        Ok(())
    } else {
        Err(Error::Sha512Mismatch {
            url: url.to_owned(),
            expected: expected.to_owned(),
            actual: actual.to_owned(),
        })
    }
}

/// Step 4: the last moment before the cache directory changes.
fn fail_if_interrupted() -> Result<(), Error> {
    match interruption::received() {
        Some(signal) => Err(Error::Interrupted(signal)),
        None => Ok(()),
    }
}

/// Step 5 (fetch-4).
fn find_sha256_directory(cache: &Path, sha256: &str) -> Result<Option<PathBuf>, Error> {
    let mut found: Vec<PathBuf> = sha256_directories(cache)?
        .into_iter()
        .filter(|directory| directory.ends_with(sha256))
        .collect();
    match found.len() {
        0 | 1 => Ok(found.pop()),
        _ => Err(Error::DuplicateSha256 {
            sha256: sha256.to_owned(),
            paths: found,
        }),
    }
}

/// Step 6 (fetch-4), up to the message: the jar of the SHA-256 directory `directory`, which
/// `AlreadyCached::record_fetch` makes record this fetch.
fn already_cached(directory: &Path, fetched: &Fetched) -> Result<Stored, Error> {
    Ok(Stored::AlreadyCached(AlreadyCached {
        cached_jar: cached_jar(directory, read_source_toml(directory)?)?,
        source: fetched.source.to_owned(),
        modrinth: fetched.modrinth.clone(),
    }))
}

/// Step 7: the SHA-256 directory appears by renaming a working directory that already holds both
/// files (d-pcache-1).
fn store_new(
    cache: &Path,
    file: UnnamedFile,
    sha256: &str,
    fetched: &Fetched,
) -> Result<Stored, Error> {
    let plugin_directory = plugin_directory(cache, fetched.plugin_name);
    let working_directory = create_working_directory(&plugin_directory)?;
    let source_toml = fill_working_directory(&working_directory, file, fetched)
        .map_err(|failure| after_removing_working_directory(&working_directory, failure))?;
    let sha256_directory = plugin_directory.join(sha256);
    match atomic_fs::rename_without_replace(&working_directory, &sha256_directory) {
        Ok(()) => Ok(Stored::New(cached_jar(&sha256_directory, source_toml)?)),
        // Another mism stored the same jar under the same plugin name meanwhile.
        Err(atomic_fs::Error::AlreadyExists { .. }) => {
            remove_working_directory(&working_directory)?;
            already_cached(&sha256_directory, fetched)
        }
        Err(error) => Err(after_removing_working_directory(
            &working_directory,
            error.into(),
        )),
    }
}

/// `<cache>/plugins/<the plugin name with each / replaced by ->`.
fn plugin_directory(cache: &Path, plugin_name: &str) -> PathBuf {
    cache
        .join(PLUGINS_DIRECTORY_NAME)
        .join(plugin_name.replace('/', "-"))
}

fn create_working_directory(plugin_directory: &Path) -> Result<PathBuf, Error> {
    create_directory(plugin_directory)?;
    let working_directory = plugin_directory.join(atomic_fs::working_name()?);
    fs::create_dir(&working_directory).map_err(|source| Error::CreateDirectory {
        path: working_directory.clone(),
        source,
    })?;
    Ok(working_directory)
}

fn fill_working_directory(
    working_directory: &Path,
    file: UnnamedFile,
    fetched: &Fetched,
) -> Result<SourceToml, Error> {
    file.link_as(&working_directory.join(fetched.file_name))?;
    let source_toml = SourceToml {
        name: fetched.plugin_name.to_owned(),
        source: fetched.source.to_owned(),
        file_name: fetched.file_name.to_owned(),
        modrinth: fetched.modrinth.clone(),
    };
    write_source_toml(working_directory, &source_toml)?;
    Ok(source_toml)
}

fn remove_working_directory(working_directory: &Path) -> Result<(), Error> {
    fs::remove_dir_all(working_directory).map_err(|source| Error::RemoveWorkingDirectory {
        path: working_directory.to_path_buf(),
        source,
    })
}

/// Removes the working directory after `failure`, and returns the error to report.
fn after_removing_working_directory(working_directory: &Path, failure: Error) -> Error {
    match fs::remove_dir_all(working_directory) {
        Ok(()) => failure,
        Err(removal) => Error::LeftWorkingDirectory {
            path: working_directory.to_path_buf(),
            removal,
            failure: Box::new(failure),
        },
    }
}

fn create_directory(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path).map_err(|source| Error::CreateDirectory {
        path: path.to_path_buf(),
        source,
    })
}

fn find_by_source_in(cache: &Path, source: &str) -> Result<Option<CachedJar>, Error> {
    let found: Vec<CachedJar> = list_in(cache)?
        .into_iter()
        .filter(|cached_jar| cached_jar.source == source)
        .collect();
    only_one(found, source)
}

fn find_by_content_in(cache: &Path, path: &Path, source: &str) -> Result<Option<CachedJar>, Error> {
    let sha256 = content_hash::sha256_of_file(path).map_err(Error::HashLocalJar)?;
    let mut found = Vec::new();
    for directory in sha256_directories(cache)? {
        if directory.ends_with(&sha256) {
            let cached_jar = cached_jar(&directory, read_source_toml(&directory)?)?;
            found.push(CachedJar {
                source: source.to_owned(),
                modrinth: None,
                ..cached_jar
            });
        }
    }
    only_one(found, source)
}

fn only_one(mut found: Vec<CachedJar>, source: &str) -> Result<Option<CachedJar>, Error> {
    match found.len() {
        0 | 1 => Ok(found.pop()),
        _ => Err(Error::SeveralJars {
            fetched_from: source.to_owned(),
            paths: found
                .into_iter()
                .map(|cached_jar| cached_jar.path)
                .collect(),
        }),
    }
}

fn list_in(cache: &Path) -> Result<Vec<CachedJar>, Error> {
    let mut cached_jars = Vec::new();
    for directory in sha256_directories(cache)? {
        let source_toml = read_source_toml(&directory)?;
        cached_jars.push(cached_jar(&directory, source_toml)?);
    }
    cached_jars
        .sort_by(|left, right| (&left.name, &left.sha256).cmp(&(&right.name, &right.sha256)));
    Ok(cached_jars)
}

/// Every directory `<cache>/plugins/<directory>/<64 lowercase hexadecimal digits>`. Nothing else
/// under `<cache>/plugins/` is read.
fn sha256_directories(cache: &Path) -> Result<Vec<PathBuf>, Error> {
    let plugins = cache.join(PLUGINS_DIRECTORY_NAME);
    let mut directories = Vec::new();
    for plugin_directory in subdirectories(&plugins)? {
        directories.extend(
            subdirectories(&plugin_directory)?
                .into_iter()
                .filter(|directory| is_sha256_name(directory)),
        );
    }
    directories.sort();
    Ok(directories)
}

/// The directories directly in `directory`, none when it does not exist.
fn subdirectories(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let list_failed = |source| Error::ListDirectory {
        path: directory.to_path_buf(),
        source,
    };
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(list_failed(error)),
    };
    let mut subdirectories = Vec::new();
    for entry in entries {
        let path = entry.map_err(list_failed)?.path();
        if path.is_dir() {
            subdirectories.push(path);
        }
    }
    Ok(subdirectories)
}

fn is_sha256_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.len() == SHA256_HEX_LENGTH
                && name
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        })
}

fn read_source_toml(directory: &Path) -> Result<SourceToml, Error> {
    let path = directory.join(SOURCE_TOML_FILE_NAME);
    let text = fs::read_to_string(&path).map_err(|source| Error::ReadSourceToml {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| Error::ParseSourceToml { path, source })
}

fn write_source_toml(directory: &Path, source_toml: &SourceToml) -> Result<(), Error> {
    let path = directory.join(SOURCE_TOML_FILE_NAME);
    let text = toml::to_string(source_toml)
        .expect("strings and a table of arrays of strings are representable in TOML");
    atomic_fs::replace(&path, text.as_bytes())
        .map_err(|source| Error::WriteSourceToml { path, source })
}

/// The jar of the SHA-256 directory `directory`, whose `source.toml` is `source_toml`.
fn cached_jar(directory: &Path, source_toml: SourceToml) -> Result<CachedJar, Error> {
    file_name::check_jar_file_name(&source_toml.file_name).map_err(|source| {
        Error::RecordedFileName {
            path: directory.join(SOURCE_TOML_FILE_NAME),
            source,
        }
    })?;
    Ok(CachedJar {
        path: directory.join(&source_toml.file_name),
        sha256: directory
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a SHA-256 directory has a hexadecimal name")
            .to_owned(),
        name: source_toml.name,
        source: source_toml.source,
        file_name: source_toml.file_name,
        modrinth: source_toml.modrinth,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use sha2::{Digest, Sha256, Sha512};
    use tempfile::TempDir;
    use tokio::runtime::Runtime;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const JAR_BYTES: &[u8] = b"pretend this is Example.jar";
    const OTHER_JAR_BYTES: &[u8] = b"pretend this is another Example.jar";

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn hex(digest: &[u8]) -> String {
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn sha512_hex(bytes: &[u8]) -> String {
        hex(&Sha512::digest(bytes))
    }

    fn client() -> HttpClient {
        HttpClient::new().unwrap()
    }

    /// The jar a fetch ends with, as `mism plugin fetch` takes it.
    fn fetched_jar(stored: Stored) -> CachedJar {
        match stored {
            Stored::New(cached_jar) => cached_jar,
            Stored::AlreadyCached(already_cached) => already_cached.record_fetch().unwrap(),
        }
    }

    /// A temporary cache directory and a directory for local jar files.
    struct Directories {
        cache: TempDir,
        local: TempDir,
    }

    impl Directories {
        fn new() -> Directories {
            Directories {
                cache: temporary_directory(),
                local: temporary_directory(),
            }
        }

        fn cache(&self) -> &Path {
            self.cache.path()
        }

        /// Writes a local jar file and returns its path.
        fn local_jar(&self, name: &str, content: &[u8]) -> PathBuf {
            let path = self.local.path().join(name);
            fs::write(&path, content).unwrap();
            path
        }

        /// Stores the local jar file at `path` as the plugin `plugin_name`, and returns the jar the
        /// fetch ends with.
        fn store_local(&self, path: &Path, plugin_name: &str) -> CachedJar {
            fetched_jar(self.try_store_local(path, plugin_name).unwrap())
        }

        fn try_store_local(&self, path: &Path, plugin_name: &str) -> Result<Stored, Error> {
            let source = path.to_str().unwrap();
            store_in(
                self.cache(),
                &client(),
                &Fetched {
                    origin: Origin::File(path),
                    plugin_name,
                    file_name: path.file_name().unwrap().to_str().unwrap(),
                    source,
                    modrinth: None,
                },
            )
        }
    }

    /// A local HTTP server serving jars, alive as long as this value.
    struct FakeServer {
        runtime: Runtime,
        server: MockServer,
    }

    impl FakeServer {
        fn start() -> FakeServer {
            let runtime = Runtime::new().unwrap();
            let server = runtime.block_on(MockServer::start());
            FakeServer { runtime, server }
        }

        /// Serves `content` at `/<name>` from now on and returns its URL.
        fn serve(&self, name: &str, content: &[u8]) -> String {
            self.runtime.block_on(async {
                self.server.reset().await;
                Mock::given(method("GET"))
                    .and(path(format!("/{name}")))
                    .respond_with(ResponseTemplate::new(200).set_body_bytes(content.to_vec()))
                    .mount(&self.server)
                    .await;
            });
            format!("{}/{name}", self.server.uri())
        }
    }

    fn luckperms_record() -> ModrinthRecord {
        ModrinthRecord {
            loaders: vec!["paper".to_owned(), "bukkit".to_owned()],
            game_versions: vec!["1.9".to_owned(), "26.3".to_owned()],
            required_dependencies: Vec::new(),
        }
    }

    fn store_from_modrinth(
        cache: &Path,
        url: &str,
        published_sha512: &str,
    ) -> Result<Stored, Error> {
        store_in(
            cache,
            &client(),
            &Fetched {
                origin: Origin::Modrinth {
                    url,
                    sha512: published_sha512,
                },
                plugin_name: "LuckPerms",
                file_name: "LuckPerms-Bukkit-5.5.71.jar",
                source: "modrinth:luckperms:v5.5.71-bukkit",
                modrinth: Some(luckperms_record()),
            },
        )
    }

    fn store_from_url(cache: &Path, url: &str) -> Result<Stored, Error> {
        store_in(
            cache,
            &client(),
            &Fetched {
                origin: Origin::Url(url),
                plugin_name: "Example",
                file_name: "Example.jar",
                source: url,
                modrinth: None,
            },
        )
    }

    /// The paths of every file and directory under `directory`, relative to it.
    fn entries_under(directory: &Path) -> BTreeSet<PathBuf> {
        let mut entries = BTreeSet::new();
        collect_entries(directory, directory, &mut entries);
        entries
    }

    fn collect_entries(root: &Path, directory: &Path, entries: &mut BTreeSet<PathBuf>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            entries.insert(path.strip_prefix(root).unwrap().to_path_buf());
            if path.is_dir() {
                collect_entries(root, &path, entries);
            }
        }
    }

    /// fetch-3: a local jar file is copied to `plugins/<name>/<SHA-256>/<file name>`.
    #[test]
    fn local_jar_is_copied_into_its_sha256_directory() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);

        let stored = directories.store_local(&local, "Example");

        assert_eq!(fs::read(&stored.path).unwrap(), fs::read(&local).unwrap());
    }

    /// 「プラグインのキャッシュ」 方法: the jar path is `<cache>/plugins/<name>/<SHA-256>/<file>`.
    #[test]
    fn jar_path_is_under_the_plugin_name_and_the_sha256() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);

        let stored = directories.store_local(&local, "Example");

        assert_eq!(
            stored.path,
            directories
                .cache()
                .join("plugins/Example")
                .join(sha256_hex(JAR_BYTES))
                .join("Example.jar")
        );
    }

    /// 「プラグインのキャッシュ」 方法: `source.toml` records the name, the source, and the file
    /// name, and Modrinth's statements for a Modrinth file.
    #[test]
    fn source_toml_records_the_fetch() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("LuckPerms-Bukkit-5.5.71.jar", JAR_BYTES);
        let stored = fetched_jar(
            store_from_modrinth(directories.cache(), &url, &sha512_hex(JAR_BYTES)).unwrap(),
        );

        let directory = stored.path.parent().unwrap().to_path_buf();

        assert_eq!(
            read_source_toml(&directory).unwrap(),
            SourceToml {
                name: "LuckPerms".to_owned(),
                source: "modrinth:luckperms:v5.5.71-bukkit".to_owned(),
                file_name: "LuckPerms-Bukkit-5.5.71.jar".to_owned(),
                modrinth: Some(luckperms_record()),
            }
        );
    }

    /// fetch-1: a download whose SHA-512 differs from Modrinth's is an error.
    #[test]
    fn modrinth_download_with_another_sha512_is_an_error() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("LuckPerms-Bukkit-5.5.71.jar", JAR_BYTES);

        let result = store_from_modrinth(directories.cache(), &url, &sha512_hex(b"other"));

        assert!(matches!(result, Err(Error::Sha512Mismatch { .. })));
    }

    /// fetch-1: a download whose SHA-512 differs from Modrinth's leaves nothing in the cache.
    #[test]
    fn modrinth_download_with_another_sha512_leaves_nothing() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("LuckPerms-Bukkit-5.5.71.jar", JAR_BYTES);

        let _ = store_from_modrinth(directories.cache(), &url, &sha512_hex(b"other"));

        assert_eq!(entries_under(directories.cache()), BTreeSet::new());
    }

    /// fetch-2: a URL's jar is stored without a check.
    #[test]
    fn url_download_is_stored() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);

        let stored = fetched_jar(store_from_url(directories.cache(), &url).unwrap());

        assert_eq!(fs::read(&stored.path).unwrap(), JAR_BYTES);
    }

    /// fetch-4: the same content stored again is the jar already cached, with its name and path.
    #[test]
    fn same_content_is_the_cached_jar() {
        let directories = Directories::new();
        let first = directories.local_jar("First.jar", JAR_BYTES);
        let second = directories.local_jar("Second.jar", JAR_BYTES);
        let cached = directories.store_local(&first, "First");

        let stored = directories.try_store_local(&second, "Second").unwrap();

        assert!(
            matches!(&stored, Stored::AlreadyCached(already_cached) if already_cached.cached_jar == cached),
            "{stored:?}"
        );
    }

    /// fetch-4: recording the fetch in the jar already cached replaces only its source and
    /// Modrinth's statements.
    #[test]
    fn recorded_fetch_replaces_the_source_of_the_cached_jar() {
        let directories = Directories::new();
        let first = directories.local_jar("First.jar", JAR_BYTES);
        let second = directories.local_jar("Second.jar", JAR_BYTES);
        let cached = directories.store_local(&first, "First");

        let recorded = directories.store_local(&second, "Second");

        assert_eq!(
            (
                recorded,
                read_source_toml(cached.path.parent().unwrap())
                    .unwrap()
                    .source
            ),
            (
                CachedJar {
                    source: second.to_str().unwrap().to_owned(),
                    ..cached
                },
                second.to_str().unwrap().to_owned()
            )
        );
    }

    /// fetch-4: storing the same content again adds no SHA-256 directory.
    #[test]
    fn same_content_adds_no_directory() {
        let directories = Directories::new();
        let first = directories.local_jar("First.jar", JAR_BYTES);
        let second = directories.local_jar("Second.jar", JAR_BYTES);
        directories.store_local(&first, "First");

        directories.store_local(&second, "Second");

        assert_eq!(sha256_directories(directories.cache()).unwrap().len(), 1);
    }

    /// fetch-4, place-1: after the same jar is fetched again, its new source finds it.
    #[test]
    fn new_source_of_a_refetched_jar_finds_it() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        directories.store_local(&local, "Example");
        let stored = fetched_jar(store_from_url(directories.cache(), &url).unwrap());

        let found = find_by_source_in(directories.cache(), &url).unwrap();

        assert_eq!(found.as_ref(), Some(&stored));
    }

    /// fetch-4, place-1: after the same jar is fetched again, its previous URL source does not
    /// find it.
    #[test]
    fn previous_url_source_of_a_refetched_jar_does_not_find_it() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);
        fetched_jar(store_from_url(directories.cache(), &url).unwrap());
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        directories.store_local(&local, "Example");

        assert_eq!(find_by_source_in(directories.cache(), &url).unwrap(), None);
    }

    /// d-pcache-1: while the working directory is being filled, no SHA-256 directory exists.
    #[test]
    fn no_sha256_directory_exists_before_the_rename() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let (file, _) =
            receive_verified(directories.cache(), &client(), Origin::File(&local)).unwrap();
        let fetched = Fetched {
            origin: Origin::File(&local),
            plugin_name: "Example",
            file_name: "Example.jar",
            source: "Example.jar",
            modrinth: None,
        };

        let working_directory =
            create_working_directory(&plugin_directory(directories.cache(), "Example")).unwrap();
        fill_working_directory(&working_directory, file, &fetched).unwrap();

        assert_eq!(
            sha256_directories(directories.cache()).unwrap(),
            Vec::<PathBuf>::new()
        );
    }

    /// d-pcache-1: a stored SHA-256 directory holds both the jar and `source.toml`.
    #[test]
    fn sha256_directory_holds_the_jar_and_source_toml() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);

        directories.store_local(&local, "Example");

        let sha256_directory = PathBuf::from("plugins/Example").join(sha256_hex(JAR_BYTES));
        assert_eq!(
            entries_under(directories.cache()),
            BTreeSet::from([
                PathBuf::from("plugins"),
                PathBuf::from("plugins/Example"),
                sha256_directory.clone(),
                sha256_directory.join("Example.jar"),
                sha256_directory.join("source.toml"),
            ])
        );
    }

    /// d-pcache-2
    #[test]
    fn source_of_two_different_jars_is_an_error_naming_both() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);
        let first = fetched_jar(store_from_url(directories.cache(), &url).unwrap());
        server.serve("Example.jar", OTHER_JAR_BYTES);
        let second = fetched_jar(store_from_url(directories.cache(), &url).unwrap());

        let error = find_by_source_in(directories.cache(), &url).unwrap_err();

        let reported = match error {
            Error::SeveralJars { paths, .. } => BTreeSet::from_iter(paths),
            other => panic!("unexpected error: {other}"),
        };
        assert_eq!(
            reported,
            BTreeSet::from([first.path.clone(), second.path.clone()])
        );
    }

    /// d-pcache-3
    #[test]
    fn slash_in_the_plugin_name_becomes_a_hyphen_in_the_directory_only() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);

        let stored = directories.store_local(&local, "A/B");

        let plugin_directory = stored.path.parent().unwrap().parent().unwrap();
        assert_eq!(
            (
                plugin_directory.file_name().unwrap().to_str().unwrap(),
                list_in(directories.cache()).unwrap()[0].name.as_str()
            ),
            ("A-B", "A/B")
        );
    }

    /// place-1: a local jar source finds the cached jar with the same content, as fetched with
    /// that source and without Modrinth's statements.
    #[test]
    fn local_jar_source_finds_the_jar_with_the_same_content() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("LuckPerms-Bukkit-5.5.71.jar", JAR_BYTES);
        let stored = fetched_jar(
            store_from_modrinth(directories.cache(), &url, &sha512_hex(JAR_BYTES)).unwrap(),
        );
        let local = directories.local_jar("copy.jar", JAR_BYTES);

        let found = find_by_content_in(directories.cache(), &local, "copy.jar").unwrap();

        assert_eq!(
            found,
            Some(CachedJar {
                source: "copy.jar".to_owned(),
                modrinth: None,
                ..stored.clone()
            })
        );
    }

    /// place-1: a source that nothing was fetched with finds nothing.
    #[test]
    fn unknown_source_finds_nothing() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        directories.store_local(&local, "Example");

        assert_eq!(
            find_by_source_in(directories.cache(), "https://example.com/Example.jar").unwrap(),
            None
        );
    }

    /// list-3, d-list-1: the jars are listed by name and then by SHA-256.
    #[test]
    fn list_is_ordered_by_name_and_sha256() {
        let directories = Directories::new();
        let contents: [&[u8]; 3] = [b"b one", b"a", b"b two"];
        for (index, (name, content)) in ["B", "A", "B"].into_iter().zip(contents).enumerate() {
            let local = directories.local_jar(&format!("{index}.jar"), content);
            directories.store_local(&local, name);
        }

        let listed: Vec<_> = list_in(directories.cache())
            .unwrap()
            .into_iter()
            .map(|cached| (cached.name, cached.sha256))
            .collect();

        let mut b_hashes = [sha256_hex(b"b one"), sha256_hex(b"b two")];
        b_hashes.sort();
        let [first_b, second_b] = b_hashes;
        assert_eq!(
            listed,
            [
                ("A".to_owned(), sha256_hex(b"a")),
                ("B".to_owned(), first_b),
                ("B".to_owned(), second_b),
            ]
        );
    }

    /// list-3: without `plugins/` in the cache directory the list is empty.
    #[test]
    fn list_without_plugins_directory_is_empty() {
        let directories = Directories::new();

        assert_eq!(list_in(directories.cache()).unwrap(), []);
    }

    /// 「プラグインのキャッシュ」 方法: only directories with SHA-256 names on the second level are
    /// read.
    #[test]
    fn other_entries_under_plugins_are_not_read() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let stored = directories.store_local(&local, "Example");
        let plugins = directories.cache().join("plugins");
        fs::write(plugins.join("notes.txt"), "not a plugin").unwrap();
        fs::create_dir(plugins.join("Example/.mism-tmp-0123456789abcdef")).unwrap();
        fs::create_dir(plugins.join("Example").join("A".repeat(64))).unwrap();

        assert_eq!(list_in(directories.cache()).unwrap(), [stored]);
    }

    /// 「プラグインのキャッシュ」 方法: an invalid `source.toml` is an error whose hint is to
    /// remove its directory.
    #[test]
    fn invalid_source_toml_is_an_error_with_a_hint_to_remove_its_directory() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let stored = directories.store_local(&local, "Example");
        let directory = stored.path.parent().unwrap().to_path_buf();
        fs::write(
            directory.join("source.toml"),
            "name = \"Example\"\nfile_name = \"Example.jar\"\n",
        )
        .unwrap();

        let error = list_in(directories.cache()).unwrap_err();

        assert_eq!(
            error.hint(),
            Some(format!(
                "remove the directory {} and fetch the plugin again",
                directory.display()
            ))
        );
    }

    /// 「プラグインのキャッシュ」 方法: an unknown field in `source.toml` is an error.
    #[test]
    fn unknown_field_in_source_toml_is_an_error() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let stored = directories.store_local(&local, "Example");
        let path = stored.path.with_file_name("source.toml");
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{text}version = \"1.0\"\n")).unwrap();

        assert!(matches!(
            list_in(directories.cache()),
            Err(Error::ParseSourceToml { .. })
        ));
    }

    /// 「ファイルの名前の検査」 方法: a `file_name` in `source.toml` that is not a file name is an
    /// error.
    #[test]
    fn recorded_file_name_with_a_slash_is_an_error() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let stored = directories.store_local(&local, "Example");
        let path = stored.path.with_file_name("source.toml");
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text.replace("\"Example.jar\"", "\"../Example.jar\"")).unwrap();

        assert!(matches!(
            list_in(directories.cache()),
            Err(Error::RecordedFileName { .. })
        ));
    }

    /// plg-6: a plugin name that is `..` is rejected before anything is stored.
    #[test]
    fn plugin_name_dot_dot_stores_nothing() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);

        let _ = directories.try_store_local(&local, "..");

        assert_eq!(entries_under(directories.cache()), BTreeSet::new());
    }
}
