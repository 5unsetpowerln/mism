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
use crate::modrinth_source;

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
        // Boxed: the parser's error is large, and every result that carries this error carries
        // its size.
        #[source]
        source: Box<toml::de::Error>,
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
    CopyLocalFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    HashLocalFile(content_hash::Error),
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
    #[error(
        "several directories of the cache directory hold the file with the SHA-256 {sha256}: {}",
        list_paths(.paths)
    )]
    DuplicateSha256 { sha256: String, paths: Vec<PathBuf> },
    #[error(
        "the cache directory has several files fetched from {fetched_from}: {}",
        list_paths(.paths)
    )]
    SeveralFiles {
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

/// The paths separated by `, `, as the messages about several files in the cache directory list
/// them.
pub fn list_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl Error {
    /// None for the files that cannot be used and for several files of one source: their remedies
    /// name what the caller fetches.
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Sha512Mismatch { .. } => Some(
                "run the command again; if this repeats, check the file on Modrinth".to_owned(),
            ),
            Error::CacheDirectory(error) => error.hint(),
            Error::FileName(error) => error.hint(),
            Error::HashLocalFile(error) => error.hint(),
            Error::Download(error) => error.hint(),
            Error::AtomicFs(error) => error.hint(),
            Error::LeftWorkingDirectory { failure, .. } => failure.hint(),
            Error::CreateDirectory { .. }
            | Error::ListDirectory { .. }
            | Error::ReadSourceToml { .. }
            | Error::ParseSourceToml { .. }
            | Error::RecordedFileName { .. }
            | Error::WriteSourceToml { .. }
            | Error::CopyLocalFile { .. }
            | Error::Interrupted(_)
            | Error::DuplicateSha256 { .. }
            | Error::SeveralFiles { .. }
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
    /// Only when the file was fetched from Modrinth.
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

/// A file in the cache directory, as its `source.toml` records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedFile {
    /// The name of what the file is (plg-5, dp-8).
    pub name: String,
    pub source: String,
    pub file_name: String,
    /// The name of its SHA-256 directory.
    pub sha256: String,
    /// The absolute path of the file.
    pub path: PathBuf,
    pub modrinth: Option<ModrinthRecord>,
}

impl CachedFile {
    /// The Modrinth version number in its source, none unless the source is Modrinth (fetch-5,
    /// insp-2, place-5, list-4, dfetch-5, dinsp-2, dplace-5, dlist-4).
    pub fn version_number(&self) -> Option<&str> {
        modrinth_source::version_number_of(&self.source)
    }
}

/// Where the content of a file to store comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A Modrinth file, checked against the SHA-512 Modrinth publishes (fetch-1, dfetch-1).
    Modrinth { url: String, sha512: String },
    /// A URL given by the user (fetch-2, dfetch-2).
    Url(String),
    /// A local file (fetch-3, dfetch-3).
    File(PathBuf),
}

/// A file to store and what to record about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub origin: Origin,
    /// The name of what the file is (plg-5, dp-8).
    pub name: String,
    pub file_name: String,
    /// The source as `source.toml` records it.
    pub source: String,
    pub modrinth: Option<ModrinthRecord>,
}

/// The result of storing a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// A new SHA-256 directory holds the file.
    New(CachedFile),
    /// The cache directory already holds the same file (fetch-4, dfetch-4).
    AlreadyCached(AlreadyCached),
}

/// The file already in the cache directory with the content just fetched, whose `source.toml`
/// does not record this fetch yet. The caller writes the fetch-4 (or dfetch-4) message with its
/// name and path, and then calls `record_fetch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlreadyCached {
    /// The file as its `source.toml` records it before this fetch.
    pub cached_file: CachedFile,
    source: String,
    modrinth: Option<ModrinthRecord>,
}

impl AlreadyCached {
    /// The rest of step 6: its `source.toml` records this fetch's source and Modrinth's
    /// statements, and keeps its name and file name. Returns the file as now recorded.
    pub fn record_fetch(self) -> Result<CachedFile, Error> {
        let directory = self
            .cached_file
            .path
            .parent()
            .expect("a cached file is in its SHA-256 directory");
        let source_toml = SourceToml {
            name: self.cached_file.name.clone(),
            source: self.source,
            file_name: self.cached_file.file_name.clone(),
            modrinth: self.modrinth,
        };
        write_source_toml(directory, &source_toml)?;
        Ok(CachedFile {
            source: source_toml.source,
            modrinth: source_toml.modrinth,
            ..self.cached_file
        })
    }
}

/// The fetched files of one directory of the cache directory, `<cache>/<directory name>/`.
#[derive(Debug, Clone)]
pub struct FetchedFileCache {
    /// The name of the directory of the cache directory that holds the files.
    directory_name: &'static str,
    /// The cache directory itself, where unnamed files are made.
    cache: PathBuf,
}

impl FetchedFileCache {
    /// The files that `<cache>/<directory_name>/` holds (e.g. `plugins`).
    pub fn open(directory_name: &'static str) -> Result<FetchedFileCache, Error> {
        Ok(FetchedFileCache::in_cache_directory(
            directory_name,
            cache_directory::path()?,
        ))
    }

    fn in_cache_directory(directory_name: &'static str, cache: PathBuf) -> FetchedFileCache {
        FetchedFileCache {
            directory_name,
            cache,
        }
    }

    fn files_directory(&self) -> PathBuf {
        self.cache.join(self.directory_name)
    }

    /// Stores the file in the cache directory (fetch-1 to fetch-4, dfetch-1 to dfetch-4), after
    /// checking its names (plg-6, dp-9). The unnamed file of a file already cached is closed here.
    pub fn store(&self, client: &HttpClient, fetched: &Fetched) -> Result<Stored, Error> {
        file_name::check_name(&fetched.name)?;
        file_name::check_file_name(&fetched.file_name)?;
        let (file, sha256) = self.receive_verified(client, &fetched.origin)?;
        fail_if_interrupted()?;
        match self.find_sha256_directory(&sha256)? {
            Some(directory) => self.already_cached(&directory, fetched),
            None => self.store_new(file, &sha256, fetched),
        }
    }

    /// The file fetched with `source`: for the local file `local_file`, the file with its
    /// content, else the file fetched with exactly that source (place-1, insp-1, dplace-1,
    /// dinsp-1). `None` when there is none.
    pub fn find(
        &self,
        source: &str,
        local_file: Option<&Path>,
    ) -> Result<Option<CachedFile>, Error> {
        match local_file {
            Some(path) => self.find_by_content(path, source),
            None => self.find_by_source(source),
        }
    }

    /// The file fetched with exactly `source` (a Modrinth or URL source).
    fn find_by_source(&self, source: &str) -> Result<Option<CachedFile>, Error> {
        let found: Vec<CachedFile> = self
            .list()?
            .into_iter()
            .filter(|cached_file| cached_file.source == source)
            .collect();
        self.only_one(found, source)
    }

    /// The file with the content of the local file at `path`, taken as fetched with `source`
    /// without Modrinth's statements.
    fn find_by_content(&self, path: &Path, source: &str) -> Result<Option<CachedFile>, Error> {
        let sha256 = content_hash::sha256_of_file(path).map_err(Error::HashLocalFile)?;
        let mut found = Vec::new();
        for directory in self.sha256_directories()? {
            if directory.ends_with(&sha256) {
                let cached_file =
                    self.cached_file(&directory, self.read_source_toml(&directory)?)?;
                found.push(CachedFile {
                    source: source.to_owned(),
                    modrinth: None,
                    ..cached_file
                });
            }
        }
        self.only_one(found, source)
    }

    /// Every file of the directory, by name and then by SHA-256 (list-3, dlist-3).
    pub fn list(&self) -> Result<Vec<CachedFile>, Error> {
        let mut cached_files = Vec::new();
        for directory in self.sha256_directories()? {
            let source_toml = self.read_source_toml(&directory)?;
            cached_files.push(self.cached_file(&directory, source_toml)?);
        }
        cached_files
            .sort_by(|left, right| (&left.name, &left.sha256).cmp(&(&right.name, &right.sha256)));
        Ok(cached_files)
    }

    /// Steps 1 to 3: the content in an unnamed file in the cache directory, and its SHA-256.
    fn receive_verified(
        &self,
        client: &HttpClient,
        origin: &Origin,
    ) -> Result<(UnnamedFile, String), Error> {
        create_directory(&self.cache)?;
        let file = UnnamedFile::create_in(&self.cache)?;
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
            Origin::File(path) => copy_local_file(path, file)?,
        };
        if let Origin::Modrinth { url, sha512 } = origin {
            verify_sha512(url, sha512, hashed.sha512.as_deref())?;
        }
        Ok((hashed.inner, hashed.sha256))
    }

    /// Step 5 (fetch-4, dfetch-4): only among the files of the directory (d-pcache-4).
    fn find_sha256_directory(&self, sha256: &str) -> Result<Option<PathBuf>, Error> {
        let mut found: Vec<PathBuf> = self
            .sha256_directories()?
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

    /// Step 6 (fetch-4, dfetch-4), up to the message: the file of the SHA-256 directory
    /// `directory`, which `AlreadyCached::record_fetch` makes record this fetch.
    fn already_cached(&self, directory: &Path, fetched: &Fetched) -> Result<Stored, Error> {
        Ok(Stored::AlreadyCached(AlreadyCached {
            cached_file: self.cached_file(directory, self.read_source_toml(directory)?)?,
            source: fetched.source.clone(),
            modrinth: fetched.modrinth.clone(),
        }))
    }

    /// Step 7: the SHA-256 directory appears by renaming a working directory that already holds
    /// both files (d-pcache-1).
    fn store_new(
        &self,
        file: UnnamedFile,
        sha256: &str,
        fetched: &Fetched,
    ) -> Result<Stored, Error> {
        let name_directory = self.name_directory(&fetched.name);
        let working_directory = create_working_directory(&name_directory)?;
        let source_toml = fill_working_directory(&working_directory, file, fetched)
            .map_err(|failure| after_removing_working_directory(&working_directory, failure))?;
        let sha256_directory = name_directory.join(sha256);
        match atomic_fs::rename_without_replace(&working_directory, &sha256_directory) {
            Ok(()) => Ok(Stored::New(
                self.cached_file(&sha256_directory, source_toml)?,
            )),
            // Another mism stored the same file under the same name meanwhile.
            Err(atomic_fs::Error::AlreadyExists { .. }) => {
                remove_working_directory(&working_directory)?;
                self.already_cached(&sha256_directory, fetched)
            }
            Err(error) => Err(after_removing_working_directory(
                &working_directory,
                error.into(),
            )),
        }
    }

    /// `<cache>/<directory name>/<the name with each / replaced by ->`.
    fn name_directory(&self, name: &str) -> PathBuf {
        self.files_directory().join(name.replace('/', "-"))
    }

    fn only_one(
        &self,
        mut found: Vec<CachedFile>,
        source: &str,
    ) -> Result<Option<CachedFile>, Error> {
        match found.len() {
            0 | 1 => Ok(found.pop()),
            _ => Err(Error::SeveralFiles {
                fetched_from: source.to_owned(),
                paths: found
                    .into_iter()
                    .map(|cached_file| cached_file.path)
                    .collect(),
            }),
        }
    }

    /// Every directory `<cache>/<directory name>/<directory>/<64 lowercase hexadecimal digits>`.
    /// Nothing else under `<cache>/<directory name>/` is read.
    fn sha256_directories(&self) -> Result<Vec<PathBuf>, Error> {
        let mut directories = Vec::new();
        for name_directory in subdirectories(&self.files_directory())? {
            directories.extend(
                subdirectories(&name_directory)?
                    .into_iter()
                    .filter(|directory| is_sha256_name(directory)),
            );
        }
        directories.sort();
        Ok(directories)
    }

    fn read_source_toml(&self, directory: &Path) -> Result<SourceToml, Error> {
        let path = directory.join(SOURCE_TOML_FILE_NAME);
        let text = fs::read_to_string(&path).map_err(|source| Error::ReadSourceToml {
            path: path.clone(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| Error::ParseSourceToml {
            path,
            source: Box::new(source),
        })
    }

    /// The file of the SHA-256 directory `directory`, whose `source.toml` is `source_toml`.
    fn cached_file(&self, directory: &Path, source_toml: SourceToml) -> Result<CachedFile, Error> {
        file_name::check_file_name(&source_toml.file_name).map_err(|source| {
            Error::RecordedFileName {
                path: directory.join(SOURCE_TOML_FILE_NAME),
                source,
            }
        })?;
        Ok(CachedFile {
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
}

fn copy_local_file(
    path: &Path,
    file: UnnamedFile,
) -> Result<content_hash::Hashed<UnnamedFile>, Error> {
    let copy_failed = |source| Error::CopyLocalFile {
        path: path.to_path_buf(),
        source,
    };
    let mut hashing = HashingWriter::new(file);
    io::copy(&mut File::open(path).map_err(copy_failed)?, &mut hashing).map_err(copy_failed)?;
    Ok(hashing.finish())
}

/// Step 3 (fetch-1, dfetch-1).
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

fn create_working_directory(name_directory: &Path) -> Result<PathBuf, Error> {
    create_directory(name_directory)?;
    let working_directory = name_directory.join(atomic_fs::working_name()?);
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
    file.link_as(&working_directory.join(&fetched.file_name))?;
    let source_toml = SourceToml {
        name: fetched.name.clone(),
        source: fetched.source.clone(),
        file_name: fetched.file_name.clone(),
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

fn write_source_toml(directory: &Path, source_toml: &SourceToml) -> Result<(), Error> {
    let path = directory.join(SOURCE_TOML_FILE_NAME);
    let text = toml::to_string(source_toml)
        .expect("strings and a table of arrays of strings are representable in TOML");
    atomic_fs::replace(&path, text.as_bytes())
        .map_err(|source| Error::WriteSourceToml { path, source })
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
    const PLUGINS: &str = "plugins";
    const DATAPACKS: &str = "datapacks";

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

    /// The file a fetch ends with, as `fetch` takes it.
    fn fetched_file(stored: Stored) -> CachedFile {
        match stored {
            Stored::New(cached_file) => cached_file,
            Stored::AlreadyCached(already_cached) => already_cached.record_fetch().unwrap(),
        }
    }

    /// A temporary cache directory and a directory for local files.
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

        /// The plugins in the temporary cache directory.
        fn plugins(&self) -> FetchedFileCache {
            self.of_directory(PLUGINS)
        }

        fn of_directory(&self, directory_name: &'static str) -> FetchedFileCache {
            FetchedFileCache::in_cache_directory(directory_name, self.cache().to_path_buf())
        }

        /// Writes a local file and returns its path.
        fn local_jar(&self, name: &str, content: &[u8]) -> PathBuf {
            let path = self.local.path().join(name);
            fs::write(&path, content).unwrap();
            path
        }

        /// Stores the local jar file at `path` as the plugin `name`, and returns the jar the
        /// fetch ends with.
        fn store_local(&self, path: &Path, name: &str) -> CachedFile {
            fetched_file(self.try_store_local(&self.plugins(), path, name).unwrap())
        }

        fn try_store_local(
            &self,
            cache: &FetchedFileCache,
            path: &Path,
            name: &str,
        ) -> Result<Stored, Error> {
            cache.store(
                &client(),
                &Fetched {
                    origin: Origin::File(path.to_path_buf()),
                    name: name.to_owned(),
                    file_name: path.file_name().unwrap().to_str().unwrap().to_owned(),
                    source: path.to_str().unwrap().to_owned(),
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
        cache: &FetchedFileCache,
        url: &str,
        published_sha512: &str,
    ) -> Result<Stored, Error> {
        cache.store(
            &client(),
            &Fetched {
                origin: Origin::Modrinth {
                    url: url.to_owned(),
                    sha512: published_sha512.to_owned(),
                },
                name: "LuckPerms".to_owned(),
                file_name: "LuckPerms-Bukkit-5.5.71.jar".to_owned(),
                source: "modrinth:luckperms:v5.5.71-bukkit".to_owned(),
                modrinth: Some(luckperms_record()),
            },
        )
    }

    fn store_from_url(cache: &FetchedFileCache, url: &str) -> Result<Stored, Error> {
        cache.store(
            &client(),
            &Fetched {
                origin: Origin::Url(url.to_owned()),
                name: "Example".to_owned(),
                file_name: "Example.jar".to_owned(),
                source: url.to_owned(),
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

    /// 「キャッシュのディレクトリのプラグインとデータパック」 方法: the jar path is
    /// `<cache>/plugins/<name>/<SHA-256>/<file>`.
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

    /// 「キャッシュのディレクトリのプラグインとデータパック」 方法: a datapack file's path is
    /// `<cache>/datapacks/<name>/<SHA-256>/<file>`.
    #[test]
    fn datapack_path_is_under_datapacks() {
        let directories = Directories::new();
        let local = directories.local_jar("Pack.zip", JAR_BYTES);

        let stored = fetched_file(
            directories
                .try_store_local(&directories.of_directory(DATAPACKS), &local, "Pack")
                .unwrap(),
        );

        assert_eq!(
            stored.path,
            directories
                .cache()
                .join("datapacks/Pack")
                .join(sha256_hex(JAR_BYTES))
                .join("Pack.zip")
        );
    }

    /// 「キャッシュのディレクトリのプラグインとデータパック」 方法: `source.toml` records the name,
    /// the source, and the file name, and Modrinth's statements for a Modrinth file.
    #[test]
    fn source_toml_records_the_fetch() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("LuckPerms-Bukkit-5.5.71.jar", JAR_BYTES);
        let cache = directories.plugins();
        let stored =
            fetched_file(store_from_modrinth(&cache, &url, &sha512_hex(JAR_BYTES)).unwrap());

        let directory = stored.path.parent().unwrap().to_path_buf();

        assert_eq!(
            cache.read_source_toml(&directory).unwrap(),
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

        let result = store_from_modrinth(&directories.plugins(), &url, &sha512_hex(b"other"));

        assert!(matches!(result, Err(Error::Sha512Mismatch { .. })));
    }

    /// fetch-1: a download whose SHA-512 differs from Modrinth's leaves nothing in the cache.
    #[test]
    fn modrinth_download_with_another_sha512_leaves_nothing() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("LuckPerms-Bukkit-5.5.71.jar", JAR_BYTES);

        let _ = store_from_modrinth(&directories.plugins(), &url, &sha512_hex(b"other"));

        assert_eq!(entries_under(directories.cache()), BTreeSet::new());
    }

    /// fetch-2: a URL's jar is stored without a check.
    #[test]
    fn url_download_is_stored() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);

        let stored = fetched_file(store_from_url(&directories.plugins(), &url).unwrap());

        assert_eq!(fs::read(&stored.path).unwrap(), JAR_BYTES);
    }

    /// fetch-4: the same content stored again is the jar already cached, with its name and path.
    #[test]
    fn same_content_is_the_cached_jar() {
        let directories = Directories::new();
        let first = directories.local_jar("First.jar", JAR_BYTES);
        let second = directories.local_jar("Second.jar", JAR_BYTES);
        let cached = directories.store_local(&first, "First");

        let stored = directories
            .try_store_local(&directories.plugins(), &second, "Second")
            .unwrap();

        assert!(
            matches!(&stored, Stored::AlreadyCached(already_cached) if already_cached.cached_file == cached),
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
                directories
                    .plugins()
                    .read_source_toml(cached.path.parent().unwrap())
                    .unwrap()
                    .source
            ),
            (
                CachedFile {
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

        assert_eq!(directories.plugins().sha256_directories().unwrap().len(), 1);
    }

    /// d-pcache-4
    #[test]
    fn same_content_as_a_plugin_and_as_a_datapack_has_a_sha256_directory_in_each() {
        let directories = Directories::new();
        let local = directories.local_jar("Same.zip", JAR_BYTES);
        let directory_names = [PLUGINS, DATAPACKS];
        for directory_name in directory_names {
            directories
                .try_store_local(&directories.of_directory(directory_name), &local, "Same")
                .unwrap();
        }

        let counts: Vec<_> = directory_names
            .into_iter()
            .map(|directory_name| {
                directories
                    .of_directory(directory_name)
                    .sha256_directories()
                    .unwrap()
                    .len()
            })
            .collect();

        assert_eq!(counts, [1, 1]);
    }

    /// fetch-4, place-1: after the same jar is fetched again, its new source finds it.
    #[test]
    fn new_source_of_a_refetched_jar_finds_it() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        directories.store_local(&local, "Example");
        let stored = fetched_file(store_from_url(&directories.plugins(), &url).unwrap());

        let found = directories.plugins().find(&url, None).unwrap();

        assert_eq!(found.as_ref(), Some(&stored));
    }

    /// fetch-4, place-1: after the same jar is fetched again, its previous URL source does not
    /// find it.
    #[test]
    fn previous_url_source_of_a_refetched_jar_does_not_find_it() {
        let directories = Directories::new();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);
        fetched_file(store_from_url(&directories.plugins(), &url).unwrap());
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        directories.store_local(&local, "Example");

        assert_eq!(directories.plugins().find(&url, None).unwrap(), None);
    }

    /// d-pcache-1: while the working directory is being filled, no SHA-256 directory exists.
    #[test]
    fn no_sha256_directory_exists_before_the_rename() {
        let directories = Directories::new();
        let cache = directories.plugins();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let origin = Origin::File(local.clone());
        let (file, _) = cache.receive_verified(&client(), &origin).unwrap();
        let fetched = Fetched {
            origin,
            name: "Example".to_owned(),
            file_name: "Example.jar".to_owned(),
            source: "Example.jar".to_owned(),
            modrinth: None,
        };

        let working_directory = create_working_directory(&cache.name_directory("Example")).unwrap();
        fill_working_directory(&working_directory, file, &fetched).unwrap();

        assert_eq!(cache.sha256_directories().unwrap(), Vec::<PathBuf>::new());
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
        let cache = directories.plugins();
        let server = FakeServer::start();
        let url = server.serve("Example.jar", JAR_BYTES);
        let first = fetched_file(store_from_url(&cache, &url).unwrap());
        server.serve("Example.jar", OTHER_JAR_BYTES);
        let second = fetched_file(store_from_url(&cache, &url).unwrap());

        let error = cache.find(&url, None).unwrap_err();

        let reported = match error {
            Error::SeveralFiles { paths, .. } => BTreeSet::from_iter(paths),
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

        let name_directory = stored.path.parent().unwrap().parent().unwrap();
        assert_eq!(
            (
                name_directory.file_name().unwrap().to_str().unwrap(),
                directories.plugins().list().unwrap()[0].name.as_str()
            ),
            ("A-B", "A/B")
        );
    }

    /// place-1: a local jar source finds the cached jar with the same content, as fetched with
    /// that source and without Modrinth's statements.
    #[test]
    fn local_jar_source_finds_the_jar_with_the_same_content() {
        let directories = Directories::new();
        let cache = directories.plugins();
        let server = FakeServer::start();
        let url = server.serve("LuckPerms-Bukkit-5.5.71.jar", JAR_BYTES);
        let stored =
            fetched_file(store_from_modrinth(&cache, &url, &sha512_hex(JAR_BYTES)).unwrap());
        let local = directories.local_jar("copy.jar", JAR_BYTES);

        let found = cache.find("copy.jar", Some(&local)).unwrap();

        assert_eq!(
            found,
            Some(CachedFile {
                source: "copy.jar".to_owned(),
                modrinth: None,
                ..stored.clone()
            })
        );
    }

    /// d-pcache-4: a datapack source does not find a plugin with the same content.
    #[test]
    fn datapack_source_does_not_find_a_plugin_with_the_same_content() {
        let directories = Directories::new();
        let local = directories.local_jar("Same.zip", JAR_BYTES);
        directories.store_local(&local, "Same");

        assert_eq!(
            directories
                .of_directory(DATAPACKS)
                .find("Same.zip", Some(&local))
                .unwrap(),
            None
        );
    }

    /// place-1: a source that nothing was fetched with finds nothing.
    #[test]
    fn unknown_source_finds_nothing() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        directories.store_local(&local, "Example");

        assert_eq!(
            directories
                .plugins()
                .find("https://example.com/Example.jar", None)
                .unwrap(),
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

        let listed: Vec<_> = directories
            .plugins()
            .list()
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

        assert_eq!(directories.plugins().list().unwrap(), []);
    }

    /// 「キャッシュのディレクトリのプラグインとデータパック」 方法: only directories with SHA-256
    /// names on the second level are read.
    #[test]
    fn other_entries_under_plugins_are_not_read() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let stored = directories.store_local(&local, "Example");
        let plugins = directories.cache().join("plugins");
        fs::write(plugins.join("notes.txt"), "not a plugin").unwrap();
        fs::create_dir(plugins.join("Example/.mism-tmp-0123456789abcdef")).unwrap();
        fs::create_dir(plugins.join("Example").join("A".repeat(64))).unwrap();

        assert_eq!(directories.plugins().list().unwrap(), [stored]);
    }

    /// 「キャッシュのディレクトリのプラグインとデータパック」 方法: a `source.toml` without a
    /// field is an error naming it.
    #[test]
    fn source_toml_without_a_field_is_an_error() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let stored = directories.store_local(&local, "Example");
        let path = stored.path.with_file_name("source.toml");
        fs::write(&path, "name = \"Example\"\nfile_name = \"Example.jar\"\n").unwrap();

        assert!(matches!(
            directories.plugins().list(),
            Err(Error::ParseSourceToml { path: reported, .. }) if reported == path
        ));
    }

    /// 「キャッシュのディレクトリのプラグインとデータパック」 方法: an unknown field in
    /// `source.toml` is an error.
    #[test]
    fn unknown_field_in_source_toml_is_an_error() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);
        let stored = directories.store_local(&local, "Example");
        let path = stored.path.with_file_name("source.toml");
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{text}version = \"1.0\"\n")).unwrap();

        assert!(matches!(
            directories.plugins().list(),
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
            directories.plugins().list(),
            Err(Error::RecordedFileName { .. })
        ));
    }

    /// insp-2: `version` is the Modrinth version number.
    #[test]
    fn version_is_the_version_number_of_a_modrinth_source() {
        let cached = CachedFile {
            name: "LuckPerms".to_owned(),
            source: "modrinth:luckperms:v5.5.71-bukkit".to_owned(),
            file_name: "LuckPerms-Bukkit-5.5.71.jar".to_owned(),
            sha256: "0".repeat(64),
            path: PathBuf::from("/cache/mism/plugins/LuckPerms/0/LuckPerms-Bukkit-5.5.71.jar"),
            modrinth: Some(luckperms_record()),
        };

        assert_eq!(cached.version_number(), Some("v5.5.71-bukkit"));
    }

    /// insp-2: `version` is absent unless the source is Modrinth.
    #[test]
    fn version_is_absent_for_a_url_source() {
        let cached = CachedFile {
            name: "Example".to_owned(),
            source: "https://example.com/Example.jar".to_owned(),
            file_name: "Example.jar".to_owned(),
            sha256: "0".repeat(64),
            path: PathBuf::from("/cache/mism/plugins/Example/0/Example.jar"),
            modrinth: None,
        };

        assert_eq!(cached.version_number(), None);
    }

    /// dinsp-2: `version` is the Modrinth version number, as for a plugin.
    #[test]
    fn datapack_version_is_the_version_number_of_a_modrinth_source() {
        let cached = CachedFile {
            name: "True Ending - Ender Dragon Overhaul".to_owned(),
            source: "modrinth:true-ending:1.1.4d".to_owned(),
            file_name: "tru.e-ending-1.1.4d.zip".to_owned(),
            sha256: "0".repeat(64),
            path: PathBuf::from("/cache/mism/datapacks/True Ending/0/tru.e-ending-1.1.4d.zip"),
            modrinth: None,
        };

        assert_eq!(cached.version_number(), Some("1.1.4d"));
    }

    /// plg-6: a plugin name that is `..` is rejected before anything is stored.
    #[test]
    fn plugin_name_dot_dot_stores_nothing() {
        let directories = Directories::new();
        let local = directories.local_jar("Example.jar", JAR_BYTES);

        let _ = directories.try_store_local(&directories.plugins(), &local, "..");

        assert_eq!(entries_under(directories.cache()), BTreeSet::new());
    }
}
