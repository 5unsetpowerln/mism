use std::fmt;
use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::ServerDirectory;
use super::mism_toml::{self, MismToml, Record};
use crate::atomic_fs::{self, UnnamedFile};
use crate::content_hash::{self, HashingWriter};
use crate::file_name;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to list {}", path.display())]
    ListDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to examine {}", path.display())]
    Examine {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the name of {} is not UTF-8", path.display())]
    FileNameNotUtf8 { path: PathBuf },
    #[error("mism.toml already has a record named {name}")]
    RecordExists { name: String },
    #[error("{} already exists", path.display())]
    FileExists { path: PathBuf },
    #[error("{} is not a directory", path.display())]
    NotDirectory { path: PathBuf },
    #[error("failed to create {}", path.display())]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to copy {} into {}", from.display(), to.display())]
    CopyFile {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "the SHA-256 of {} is {actual}, not the name of its directory in the cache directory",
        path.display()
    )]
    CachedFileSha256Mismatch { path: PathBuf, actual: String },
    #[error(transparent)]
    AtomicFs(#[from] atomic_fs::Error),
    #[error(transparent)]
    MismToml(#[from] mism_toml::Error),
    #[error("mism.toml has no record named {name}")]
    RecordNotFound { name: String },
    #[error("mism.toml has several records named {name}")]
    SeveralRecords { name: String },
    #[error("the file name in the record of {name} cannot be used")]
    RecordedFileName {
        name: String,
        #[source]
        source: file_name::Error,
    },
    #[error(transparent)]
    HashFile(content_hash::Error),
    #[error("failed to remove {}, to which the removed file was renamed", path.display())]
    RemoveRenamedFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to undo a change to {} ({undo}) after this failure", path.display())]
    Undo {
        path: PathBuf,
        undo: String,
        #[source]
        failure: Box<Error>,
    },
}

impl Error {
    /// None for the records, the files in the way, and a changed file of the cache directory:
    /// their remedies name the caller's subcommands, directory, and files.
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::AtomicFs(error) => error.hint(),
            Error::MismToml(error) => error.hint(),
            Error::HashFile(error) => error.hint(),
            Error::Undo { failure, .. } => failure.hint(),
            Error::ListDirectory { .. }
            | Error::Examine { .. }
            | Error::FileNameNotUtf8 { .. }
            | Error::RecordExists { .. }
            | Error::FileExists { .. }
            | Error::NotDirectory { .. }
            | Error::CreateDirectory { .. }
            | Error::CopyFile { .. }
            | Error::CachedFileSha256Mismatch { .. }
            | Error::RecordNotFound { .. }
            | Error::SeveralRecords { .. }
            | Error::RecordedFileName { .. }
            | Error::RemoveRenamedFile { .. } => None,
        }
    }

    /// The failure itself, behind the undos that failed after it.
    pub fn failure(&self) -> &Error {
        match self {
            Error::Undo { failure, .. } => failure.failure(),
            failure => failure,
        }
    }
}

/// The files placed in one directory of a server and their records in `mism.toml`, which the
/// caller tells: where the directory is, which files in it count, and which array of `mism.toml`
/// records them.
#[derive(Debug, Clone)]
pub struct PlacedFiles<'a> {
    pub server: &'a ServerDirectory,
    /// The absolute path of the directory the files are placed in.
    pub directory: PathBuf,
    /// The end of the names of the files that count as placed without a record (list-2,
    /// dlist-2).
    pub suffix: &'static str,
    /// The records of the files in `directory`, in the order of `mism.toml`.
    pub records: &'a [Record],
    /// `mism.toml` with `records` replaced: how changed records are written into it.
    pub with_records: fn(&MismToml, Vec<Record>) -> MismToml,
}

/// A file in the cache directory to place, and its record (plg-4, dp-7).
#[derive(Debug, Clone, Copy)]
pub struct FileToPlace<'a> {
    /// The file in its SHA-256 directory of the cache directory.
    pub cached_path: &'a Path,
    /// The name of that SHA-256 directory.
    pub sha256: &'a str,
    /// The name of what the file is (plg-5, dp-8).
    pub name: &'a str,
    pub source: &'a str,
    pub file_name: &'a str,
}

/// What `remove` removed (rm-1, rm-3, drm-1, drm-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// The file in the placing directory and its record.
    FileAndRecord(Record),
    /// Only the record: the placing directory had no file of that name.
    RecordOnly(Record),
}

/// The state of a placed file in `list` (list-2, dlist-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ok,
    Changed,
    Missing,
    Unmanaged,
}

impl fmt::Display for State {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            State::Ok => "ok",
            State::Changed => "changed",
            State::Missing => "missing",
            State::Unmanaged => "unmanaged",
        })
    }
}

/// A record or an unrecorded file, held against each other (list-2, dlist-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub state: State,
    /// The record's name; for `unmanaged`, the file name without its suffix.
    pub name: String,
    pub file_name: String,
    /// The record's source; `None` for `unmanaged`.
    pub source: Option<String>,
}

impl PlacedFiles<'_> {
    /// The conditions to check before placing (place-3, place-4, dplace-3, dplace-4), and that the
    /// placing directory's path has nothing but directories.
    pub fn check_placeable(&self, name: &str, file_name: &str) -> Result<(), Error> {
        if self.records.iter().any(|record| record.name == name) {
            return Err(Error::RecordExists {
                name: name.to_owned(),
            });
        }
        missing_directories(&self.directory)?;
        let path = self.directory.join(file_name);
        match fs::symlink_metadata(&path) {
            Ok(_) => Err(Error::FileExists { path }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Error::Examine { path, source }),
        }
    }

    /// Places the file in the placing directory and adds its record to `mism.toml` (place-2,
    /// dplace-2). On a failure, the placing directory's path and `mism.toml` are as before (plg-8,
    /// dp-11).
    pub fn place(&self, file: &FileToPlace) -> Result<(), Error> {
        let created = create_missing_directories(&self.directory)?;
        self.place_in(file)
            .map_err(|failure| remove_created(failure, &created))
    }

    /// Steps 2 to 4, undoing step 3 when step 4 fails.
    fn place_in(&self, file: &FileToPlace) -> Result<(), Error> {
        let copy = copy_verified(&self.directory, file)?;
        let placed = self.directory.join(file.file_name);
        link_file(copy, &placed)?;
        self.add_record(file)
            .map_err(|failure| undo(failure, &placed, fs::remove_file(&placed)))
    }

    /// Step 4.
    fn add_record(&self, file: &FileToPlace) -> Result<(), Error> {
        let mut records = self.records.to_vec();
        records.push(Record {
            name: file.name.to_owned(),
            source: file.source.to_owned(),
            file_name: file.file_name.to_owned(),
            sha256: file.sha256.to_owned(),
        });
        self.write_records(records)
    }

    fn write_records(&self, records: Vec<Record>) -> Result<(), Error> {
        let mism_toml = (self.with_records)(&self.server.mism_toml, records);
        Ok(mism_toml.write(&self.server.mism_toml_path())?)
    }

    /// Removes the file of the record named `name` and the record (rm-1 to rm-3, drm-1 to
    /// drm-3). The file is renamed before `mism.toml` changes and deleted after, so that a
    /// failure to change either leaves both as before (plg-8, dp-11).
    pub fn remove(&self, name: &str) -> Result<Removed, Error> {
        let (index, record) = self.find_record(name)?;
        let renamed = self.rename_file_away(&record.file_name)?;
        let mut records = self.records.to_vec();
        records.remove(index);
        self.write_records(records)
            .map_err(|failure| match &renamed {
                Some(working_path) => undo(
                    failure,
                    working_path,
                    atomic_fs::rename_without_replace(
                        working_path,
                        &self.directory.join(&record.file_name),
                    ),
                ),
                None => failure,
            })?;
        match renamed {
            Some(working_path) => {
                delete_renamed_file(&working_path)?;
                Ok(Removed::FileAndRecord(record))
            }
            None => Ok(Removed::RecordOnly(record)),
        }
    }

    /// Step 1: the only record named `name`, whose file name is checked before use.
    fn find_record(&self, name: &str) -> Result<(usize, Record), Error> {
        let mut found = self
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| record.name == name);
        let (index, record) = found.next().ok_or_else(|| Error::RecordNotFound {
            name: name.to_owned(),
        })?;
        if found.next().is_some() {
            return Err(Error::SeveralRecords {
                name: name.to_owned(),
            });
        }
        check_recorded_file_name(record)?;
        Ok((index, record.clone()))
    }

    /// Step 2: the working name the file now has, or `None` when there is no such file (rm-3,
    /// drm-3).
    fn rename_file_away(&self, file_name: &str) -> Result<Option<PathBuf>, Error> {
        if !is_regular_file(&self.directory.join(file_name))? {
            return Ok(None);
        }
        let working_path = self.directory.join(atomic_fs::working_name()?);
        atomic_fs::rename_without_replace(&self.directory.join(file_name), &working_path)?;
        Ok(Some(working_path))
    }

    /// The records in their order, then the unrecorded files by name (list-1, list-2, dlist-1,
    /// dlist-2).
    pub fn reconcile(&self) -> Result<Vec<FileState>, Error> {
        let mut states = Vec::new();
        for record in self.records {
            states.push(FileState {
                state: self.record_state(record)?,
                name: record.name.clone(),
                file_name: record.file_name.clone(),
                source: Some(record.source.clone()),
            });
        }
        for file_name in file_names(&self.directory, self.suffix)? {
            if !self
                .records
                .iter()
                .any(|record| record.file_name == file_name)
            {
                states.push(FileState {
                    state: State::Unmanaged,
                    name: file_name
                        .strip_suffix(self.suffix)
                        .expect("an unrecorded file's name ends with the suffix")
                        .to_owned(),
                    file_name,
                    source: None,
                });
            }
        }
        Ok(states)
    }

    fn record_state(&self, record: &Record) -> Result<State, Error> {
        check_recorded_file_name(record)?;
        let path = self.directory.join(&record.file_name);
        if !is_regular_file(&path)? {
            return Ok(State::Missing);
        }
        let sha256 = content_hash::sha256_of_file(&path).map_err(Error::HashFile)?;
        Ok(if sha256 == record.sha256 {
            State::Ok
        } else {
            State::Changed
        })
    }
}

/// The names of the files directly in `directory`, in ascending order: names ending with `suffix`
/// of what is a regular file after following symbolic links. None when the directory does not
/// exist.
pub fn file_names(directory: &Path, suffix: &str) -> Result<Vec<String>, Error> {
    let list_failed = |source| Error::ListDirectory {
        path: directory.to_path_buf(),
        source,
    };
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if is_absent(&error) => return Ok(Vec::new()),
        Err(error) => return Err(list_failed(error)),
    };
    let suffix = suffix.as_bytes();
    let mut names = Vec::new();
    for entry in entries {
        let path = entry.map_err(list_failed)?.path();
        let has_suffix = path
            .file_name()
            .is_some_and(|name| name.as_bytes().ends_with(suffix));
        if has_suffix && is_regular_file(&path)? {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::FileNameNotUtf8 { path: path.clone() })?;
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Follows symbolic links; nothing at the path, a dangling link, or a path through something that
/// is not a directory, is no regular file.
fn is_regular_file(path: &Path) -> Result<bool, Error> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if is_absent(&error) => Ok(false),
        Err(source) => Err(Error::Examine {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Whether the error tells that nothing is at the path, also when a directory of the path is not
/// a directory (e.g. `world/datapacks` when `world` is a regular file).
fn is_absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

/// The directories of `path`, itself included, that do not exist, nearest to the root first. The
/// nearest one that exists must be a directory after following symbolic links, and none may be a
/// dangling symbolic link.
fn missing_directories(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut missing = Vec::new();
    for ancestor in path.ancestors() {
        let examine_failed = |source| Error::Examine {
            path: ancestor.to_path_buf(),
            source,
        };
        let not_directory = || Error::NotDirectory {
            path: ancestor.to_path_buf(),
        };
        match fs::metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => return Err(not_directory()),
            Err(error) if is_absent(&error) => match fs::symlink_metadata(ancestor) {
                // A dangling symbolic link: following it leads to no directory.
                Ok(_) => return Err(not_directory()),
                Err(error) if is_absent(&error) => missing.push(ancestor.to_path_buf()),
                Err(source) => return Err(examine_failed(source)),
            },
            Err(source) => return Err(examine_failed(source)),
        }
    }
    missing.reverse();
    Ok(missing)
}

/// Step 1 of the placement: the directories that were created, nearest to the root first. One
/// that another process created meanwhile is not among them.
fn create_missing_directories(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut created = Vec::new();
    for directory in missing_directories(path)? {
        match fs::create_dir(&directory) {
            Ok(()) => created.push(directory),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                let failure = Error::CreateDirectory {
                    path: directory,
                    source,
                };
                return Err(remove_created(failure, &created));
            }
        }
    }
    Ok(created)
}

/// Step 5: removes the created directories in the reverse order, and returns the error to report.
fn remove_created(failure: Error, created: &[PathBuf]) -> Error {
    created.iter().rev().fold(failure, |failure, directory| {
        undo(failure, directory, fs::remove_dir(directory))
    })
}

/// Step 2 (d-pdir-1): the copy is checked against the name of the cached file's SHA-256
/// directory.
fn copy_verified(directory: &Path, file: &FileToPlace) -> Result<UnnamedFile, Error> {
    let copy_failed = |source| Error::CopyFile {
        from: file.cached_path.to_path_buf(),
        to: directory.to_path_buf(),
        source,
    };
    let mut hashing = HashingWriter::new(UnnamedFile::create_in(directory)?);
    io::copy(
        &mut File::open(file.cached_path).map_err(copy_failed)?,
        &mut hashing,
    )
    .map_err(copy_failed)?;
    let hashed = hashing.finish();
    if hashed.sha256 == file.sha256 {
        Ok(hashed.inner)
    } else {
        Err(Error::CachedFileSha256Mismatch {
            path: file.cached_path.to_path_buf(),
            actual: hashed.sha256,
        })
    }
}

/// Step 3: a name that another process created after the check is place-4's (or dplace-4's)
/// error.
fn link_file(copy: UnnamedFile, placed: &Path) -> Result<(), Error> {
    copy.link_as(placed).map_err(|error| match error {
        atomic_fs::Error::AlreadyExists { path } => Error::FileExists { path },
        error => Error::AtomicFs(error),
    })
}

/// Returns `failure`, or when the undo after it failed too, an error telling both.
fn undo(failure: Error, path: &Path, undone: Result<(), impl fmt::Display>) -> Error {
    match undone {
        Ok(()) => failure,
        Err(undo) => Error::Undo {
            path: path.to_path_buf(),
            undo: undo.to_string(),
            failure: Box::new(failure),
        },
    }
}

fn check_recorded_file_name(record: &Record) -> Result<(), Error> {
    file_name::check_file_name(&record.file_name).map_err(|source| Error::RecordedFileName {
        name: record.name.clone(),
        source,
    })
}

/// Step 4 of the removal.
fn delete_renamed_file(working_path: &Path) -> Result<(), Error> {
    fs::remove_file(working_path).map_err(|source| Error::RemoveRenamedFile {
        path: working_path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn record(name: &str, file_name: &str, sha256: &str) -> Record {
        Record {
            name: name.to_owned(),
            source: format!("https://example.com/{file_name}"),
            file_name: file_name.to_owned(),
            sha256: sha256.to_owned(),
        }
    }

    /// A server directory with its `mism.toml` written, a cached file outside it, and the
    /// temporary directories holding both.
    struct Fixture {
        server: ServerDirectory,
        _server_root: TempDir,
        cache: TempDir,
    }

    impl Fixture {
        fn with_records(plugins: Vec<Record>, datapacks: Vec<Record>) -> Fixture {
            let server_root = temporary_directory();
            let server = ServerDirectory {
                path: server_root.path().to_path_buf(),
                mism_toml: MismToml {
                    software: "paper".to_owned(),
                    minecraft_version: "26.3".to_owned(),
                    software_version: 206,
                    plugins,
                    datapacks,
                },
            };
            server.mism_toml.write(&server.mism_toml_path()).unwrap();
            Fixture {
                server,
                _server_root: server_root,
                cache: temporary_directory(),
            }
        }

        fn with_plugin_records(records: Vec<Record>) -> Fixture {
            Fixture::with_records(records, Vec::new())
        }

        fn new() -> Fixture {
            Fixture::with_records(Vec::new(), Vec::new())
        }

        /// The jars in `plugins/`, recorded in `plugins` of `mism.toml`.
        fn plugins(&self) -> PlacedFiles<'_> {
            PlacedFiles {
                server: &self.server,
                directory: self.server.path.join("plugins"),
                suffix: ".jar",
                records: &self.server.mism_toml.plugins,
                with_records: |mism_toml, plugins| MismToml {
                    plugins,
                    ..mism_toml.clone()
                },
            }
        }

        /// The zip files in `directory`, recorded in `datapacks` of `mism.toml`.
        fn zips_in(&self, directory: PathBuf) -> PlacedFiles<'_> {
            PlacedFiles {
                server: &self.server,
                directory,
                suffix: ".zip",
                records: &self.server.mism_toml.datapacks,
                with_records: |mism_toml, datapacks| MismToml {
                    datapacks,
                    ..mism_toml.clone()
                },
            }
        }

        /// The zip files in `<server>/world/datapacks`.
        fn world_datapacks(&self) -> PlacedFiles<'_> {
            self.zips_in(self.server.path.join("world/datapacks"))
        }

        fn put_in(&self, directory: &PlacedFiles, file_name: &str, content: &[u8]) {
            fs::create_dir_all(&directory.directory).unwrap();
            fs::write(directory.directory.join(file_name), content).unwrap();
        }

        fn put_in_plugins(&self, file_name: &str, content: &[u8]) {
            self.put_in(&self.plugins(), file_name, content);
        }

        /// Writes `content` as a cached file in a directory named `sha256`.
        fn cached_file(&self, sha256: &str, file_name: &str, content: &[u8]) -> PathBuf {
            let directory = self.cache.path().join(sha256);
            fs::create_dir(&directory).unwrap();
            let path = directory.join(file_name);
            fs::write(&path, content).unwrap();
            path
        }

        fn cached_jar(&self, sha256: &str, content: &[u8]) -> PathBuf {
            self.cached_file(sha256, "Example.jar", content)
        }

        fn place(&self, cached_path: &Path, sha256: &str) -> Result<(), Error> {
            self.plugins().place(&FileToPlace {
                cached_path,
                sha256,
                name: "Example",
                source: "https://example.com/Example.jar",
                file_name: "Example.jar",
            })
        }

        /// Places a cached file `Pack.zip` with `content` in `directory`.
        fn place_zip_in(&self, directory: PathBuf, content: &[u8]) -> Result<(), Error> {
            let sha256 = sha256_hex(content);
            let cached_path = self.cached_file(&sha256, "Pack.zip", content);
            self.zips_in(directory).place(&FileToPlace {
                cached_path: &cached_path,
                sha256: &sha256,
                name: "Pack",
                source: "https://example.com/Pack.zip",
                file_name: "Pack.zip",
            })
        }

        fn read_mism_toml(&self) -> MismToml {
            MismToml::read(&self.server.mism_toml_path()).unwrap()
        }

        /// Every file and directory in the server directory with its content.
        fn snapshot(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
            snapshot_of(&self.server.path)
        }
    }

    /// Every file and directory under `root` with its content.
    fn snapshot_of(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        let mut entries = BTreeMap::new();
        collect(root, root, &mut entries);
        entries
    }

    fn collect(root: &Path, directory: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                entries.insert(relative, None);
                collect(root, &path, entries);
            } else {
                entries.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }

    /// Makes `directory` read-only until the value is dropped.
    struct ReadOnly<'a>(&'a Path);

    impl<'a> ReadOnly<'a> {
        fn new(directory: &'a Path) -> ReadOnly<'a> {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).unwrap();
            ReadOnly(directory)
        }
    }

    impl Drop for ReadOnly<'_> {
        fn drop(&mut self) {
            fs::set_permissions(self.0, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// place-2: the jar is placed in `plugins/`, which is created.
    #[test]
    fn placed_jar_is_in_plugins() {
        let fixture = Fixture::new();
        let content = b"jar content";
        let cached = fixture.cached_jar(&sha256_hex(content), content);

        fixture.place(&cached, &sha256_hex(content)).unwrap();

        assert_eq!(
            fs::read(fixture.plugins().directory.join("Example.jar")).unwrap(),
            content
        );
    }

    /// place-2, plg-4: the record is added at the end of `mism.toml`.
    #[test]
    fn placed_jar_is_recorded_last() {
        let existing = record("Vault", "Vault.jar", &"1".repeat(64));
        let fixture = Fixture::with_plugin_records(vec![existing.clone()]);
        let content = b"jar content";
        let cached = fixture.cached_jar(&sha256_hex(content), content);

        fixture.place(&cached, &sha256_hex(content)).unwrap();

        assert_eq!(
            fixture.read_mism_toml().plugins,
            [
                existing,
                record("Example", "Example.jar", &sha256_hex(content))
            ]
        );
    }

    /// d-pdir-1: a cached jar whose SHA-256 is not its directory's name is an error.
    #[test]
    fn cached_jar_with_another_sha256_is_an_error() {
        let fixture = Fixture::new();
        let cached = fixture.cached_jar(&sha256_hex(b"original"), b"changed");

        let result = fixture.place(&cached, &sha256_hex(b"original"));

        assert!(matches!(
            result,
            Err(Error::CachedFileSha256Mismatch { .. })
        ));
    }

    /// d-pdir-1: a cached jar whose SHA-256 is not its directory's name changes nothing.
    #[test]
    fn cached_jar_with_another_sha256_changes_nothing() {
        let fixture = Fixture::new();
        let cached = fixture.cached_jar(&sha256_hex(b"original"), b"changed");
        let before = fixture.snapshot();

        let _ = fixture.place(&cached, &sha256_hex(b"original"));

        assert_eq!(fixture.snapshot(), before);
    }

    /// plg-8: when `mism.toml` cannot be written, the placed jar is removed again.
    #[test]
    fn failed_record_removes_the_placed_jar() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.plugins().directory).unwrap();
        let content = b"jar content";
        let cached = fixture.cached_jar(&sha256_hex(content), content);
        let before = fixture.snapshot();

        let result = {
            let _read_only = ReadOnly::new(&fixture.server.path);
            fixture.place(&cached, &sha256_hex(content))
        };

        assert_eq!((result.is_err(), fixture.snapshot()), (true, before));
    }

    /// place-4: a jar placed by another process after the check is place-4's error.
    #[test]
    fn jar_that_appeared_after_the_check_is_an_error() {
        let fixture = Fixture::new();
        fixture.put_in_plugins("Example.jar", b"other");
        let content = b"jar content";
        let cached = fixture.cached_jar(&sha256_hex(content), content);

        let result = fixture.place(&cached, &sha256_hex(content));

        assert!(matches!(result, Err(Error::FileExists { .. })));
    }

    /// place-3
    #[test]
    fn record_with_the_same_name_prevents_placing() {
        let fixture =
            Fixture::with_plugin_records(vec![record("Example", "Other.jar", &"1".repeat(64))]);

        assert!(matches!(
            fixture.plugins().check_placeable("Example", "Example.jar"),
            Err(Error::RecordExists { .. })
        ));
    }

    /// place-4: anything named like the jar in `plugins/`, a dangling link too, prevents placing.
    #[test]
    fn dangling_link_with_the_jar_name_prevents_placing() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.plugins().directory).unwrap();
        symlink("missing", fixture.plugins().directory.join("Example.jar")).unwrap();

        assert!(matches!(
            fixture.plugins().check_placeable("Example", "Example.jar"),
            Err(Error::FileExists { .. })
        ));
    }

    /// 「プラグインとデータパックを置くディレクトリ」 方法: a `plugins` that is not a directory
    /// prevents placing.
    #[test]
    fn plugins_that_is_a_file_prevents_placing() {
        let fixture = Fixture::new();
        fs::write(fixture.plugins().directory, b"not a directory").unwrap();

        assert!(matches!(
            fixture.plugins().check_placeable("Example", "Example.jar"),
            Err(Error::NotDirectory { .. })
        ));
    }

    /// place-3, place-4: a new name and jar can be placed.
    #[test]
    fn new_name_and_jar_can_be_placed() {
        let fixture =
            Fixture::with_plugin_records(vec![record("Vault", "Vault.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Vault.jar", b"vault");

        assert!(
            fixture
                .plugins()
                .check_placeable("Example", "Example.jar")
                .is_ok()
        );
    }

    /// d-dpdir-1
    #[test]
    fn datapack_is_placed_in_a_world_directory_that_is_created() {
        let fixture = Fixture::new();

        fixture
            .place_zip_in(
                fixture.server.path.join("worlds/survival/datapacks"),
                b"zip content",
            )
            .unwrap();

        assert_eq!(
            fs::read(
                fixture
                    .server
                    .path
                    .join("worlds/survival/datapacks/Pack.zip")
            )
            .unwrap(),
            b"zip content"
        );
    }

    /// d-dpdir-2
    #[test]
    fn failed_record_removes_every_directory_the_placement_created() {
        let outside = temporary_directory();
        let fixture = Fixture::new();

        let result = {
            let _read_only = ReadOnly::new(&fixture.server.path);
            fixture.place_zip_in(
                outside.path().join("worlds/survival/datapacks"),
                b"zip content",
            )
        };

        assert_eq!(
            (result.is_err(), outside.path().join("worlds").exists()),
            (true, false)
        );
    }

    /// d-dpdir-3
    #[test]
    fn datapacks_that_is_a_file_prevents_placing_and_changes_nothing() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.server.path.join("world")).unwrap();
        fs::write(fixture.world_datapacks().directory, b"not a directory").unwrap();
        let before = fixture.snapshot();

        let result = fixture
            .world_datapacks()
            .check_placeable("Pack", "Pack.zip");

        assert_eq!(
            (
                matches!(result, Err(Error::NotDirectory { .. })),
                fixture.snapshot()
            ),
            (true, before)
        );
    }

    /// 「プラグインとデータパックを置くディレクトリ」 方法: a file where the world directory's parent
    /// would be prevents placing.
    #[test]
    fn file_in_the_world_directory_path_prevents_placing() {
        let fixture = Fixture::new();
        fs::write(fixture.server.path.join("worlds"), b"not a directory").unwrap();

        let error = fixture
            .zips_in(fixture.server.path.join("worlds/survival/datapacks"))
            .check_placeable("Pack", "Pack.zip")
            .unwrap_err();

        assert!(
            matches!(&error, Error::NotDirectory { path } if *path == fixture.server.path.join("worlds")),
            "{error}"
        );
    }

    /// d-dpdir-4
    #[test]
    fn directories_in_datapacks_are_not_listed() {
        let fixture = Fixture::new();
        let datapacks = fixture.world_datapacks();
        fixture.put_in(&datapacks, "Packed.zip", b"zip");
        fs::create_dir(datapacks.directory.join("Expanded")).unwrap();
        fs::create_dir(datapacks.directory.join("Expanded.zip")).unwrap();

        let names: Vec<_> = datapacks
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| state.file_name)
            .collect();

        assert_eq!(names, ["Packed.zip"]);
    }

    /// dlist-2: unrecorded files directly in `datapacks/` are `unmanaged` only when their names
    /// end with `.zip`.
    #[test]
    fn unmanaged_datapacks_are_the_zip_files() {
        let fixture = Fixture::new();
        let datapacks = fixture.world_datapacks();
        fixture.put_in(&datapacks, "Pack.zip", b"zip");
        fixture.put_in(&datapacks, "Plugin.jar", b"jar");

        let states: Vec<_> = datapacks
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| (state.state, state.name))
            .collect();

        assert_eq!(states, [(State::Unmanaged, "Pack".to_owned())]);
    }

    /// rm-1: the jar and the record are removed, other files in `plugins/` stay.
    #[test]
    fn removal_removes_the_jar_and_the_record_only() {
        let fixture =
            Fixture::with_plugin_records(vec![record("Example", "Example.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Example.jar", b"jar");
        fixture.put_in_plugins("config.yml", b"kept");

        fixture.plugins().remove("Example").unwrap();

        let expected_mism_toml = MismToml {
            plugins: Vec::new(),
            ..fixture.server.mism_toml.clone()
        };
        let mut expected = BTreeMap::from([
            (PathBuf::from("plugins"), None),
            (PathBuf::from("plugins/config.yml"), Some(b"kept".to_vec())),
        ]);
        expected.insert(
            PathBuf::from("mism.toml"),
            Some(toml::to_string(&expected_mism_toml).unwrap().into_bytes()),
        );
        assert_eq!(fixture.snapshot(), expected);
    }

    /// rm-1: the removal tells that the jar was removed.
    #[test]
    fn removal_of_a_placed_jar_reports_the_jar() {
        let removed_record = record("Example", "Example.jar", &"1".repeat(64));
        let fixture = Fixture::with_plugin_records(vec![removed_record.clone()]);
        fixture.put_in_plugins("Example.jar", b"jar");

        assert_eq!(
            fixture.plugins().remove("Example").unwrap(),
            Removed::FileAndRecord(removed_record)
        );
    }

    /// rm-2
    #[test]
    fn removal_of_an_unrecorded_name_is_an_error_and_changes_nothing() {
        let fixture =
            Fixture::with_plugin_records(vec![record("Example", "Example.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Example.jar", b"jar");
        let before = fixture.snapshot();

        let result = fixture.plugins().remove("Other");

        assert_eq!(
            (
                matches!(result, Err(Error::RecordNotFound { .. })),
                fixture.snapshot()
            ),
            (true, before)
        );
    }

    /// rm-3
    #[test]
    fn removal_without_the_jar_removes_only_the_record() {
        let removed_record = record("Example", "Example.jar", &"1".repeat(64));
        let fixture = Fixture::with_plugin_records(vec![removed_record.clone()]);

        let removed = fixture.plugins().remove("Example").unwrap();

        assert_eq!(
            (removed, fixture.read_mism_toml().plugins),
            (Removed::RecordOnly(removed_record), Vec::new())
        );
    }

    /// d-name-1
    #[test]
    fn record_file_name_outside_plugins_is_an_error_and_removes_nothing() {
        let fixture = Fixture::with_plugin_records(vec![record(
            "Example",
            "../server.properties",
            &"1".repeat(64),
        )]);
        fs::write(fixture.server.path.join("server.properties"), b"motd=A").unwrap();
        let before = fixture.snapshot();

        let result = fixture.plugins().remove("Example");

        assert_eq!(
            (
                matches!(result, Err(Error::RecordedFileName { .. })),
                fixture.snapshot()
            ),
            (true, before)
        );
    }

    /// plg-8: when `mism.toml` cannot be written, the removed jar gets its name back.
    #[test]
    fn failed_removal_of_the_record_restores_the_jar() {
        let fixture =
            Fixture::with_plugin_records(vec![record("Example", "Example.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Example.jar", b"jar");
        let before = fixture.snapshot();

        let result = {
            let _read_only = ReadOnly::new(&fixture.server.path);
            fixture.plugins().remove("Example")
        };

        assert_eq!((result.is_err(), fixture.snapshot()), (true, before));
    }

    /// d-pdir-2, list-2: the records in their order, then the unrecorded jars by name.
    #[test]
    fn reconciliation_lists_records_then_unrecorded_jars() {
        let fixture = Fixture::with_plugin_records(vec![
            record("Zeta", "Zeta.jar", &sha256_hex(b"zeta")),
            record("Alpha", "Alpha.jar", &sha256_hex(b"alpha")),
            record("Gone", "Gone.jar", &"1".repeat(64)),
        ]);
        fixture.put_in_plugins("Zeta.jar", b"zeta");
        fixture.put_in_plugins("Alpha.jar", b"changed");
        fixture.put_in_plugins("b-extra.jar", b"b");
        fixture.put_in_plugins("a-extra.jar", b"a");
        fixture.put_in_plugins("notes.txt", b"not a jar");
        fs::create_dir(fixture.plugins().directory.join("Directory.jar")).unwrap();

        let states: Vec<_> = fixture
            .plugins()
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| (state.state, state.name, state.source.is_some()))
            .collect();

        assert_eq!(
            states,
            [
                (State::Ok, "Zeta".to_owned(), true),
                (State::Changed, "Alpha".to_owned(), true),
                (State::Missing, "Gone".to_owned(), true),
                (State::Unmanaged, "a-extra".to_owned(), false),
                (State::Unmanaged, "b-extra".to_owned(), false),
            ]
        );
    }

    /// 「プラグインとデータパックを置くディレクトリ」 方法: the placement leaves nothing outside the
    /// world's `datapacks/` but the directories it needed.
    #[test]
    fn datapack_placement_creates_only_the_world_and_datapacks() {
        let outside = temporary_directory();
        let fixture = Fixture::new();

        fixture
            .place_zip_in(
                outside.path().join("worlds/survival/datapacks"),
                b"zip content",
            )
            .unwrap();

        assert_eq!(
            snapshot_of(outside.path()),
            BTreeMap::from([
                (PathBuf::from("worlds"), None),
                (PathBuf::from("worlds/survival"), None),
                (PathBuf::from("worlds/survival/datapacks"), None),
                (
                    PathBuf::from("worlds/survival/datapacks/Pack.zip"),
                    Some(b"zip content".to_vec())
                ),
            ])
        );
    }

    /// dlist-2: a recorded datapack file whose name does not end with `.zip` is held against its
    /// record like any other file.
    #[test]
    fn recorded_datapack_without_the_zip_suffix_is_ok() {
        let fixture = Fixture::with_records(
            Vec::new(),
            vec![record("Pack", "Pack.mcpack", &sha256_hex(b"zip"))],
        );
        fixture.put_in(&fixture.world_datapacks(), "Pack.mcpack", b"zip");

        let states: Vec<_> = fixture
            .world_datapacks()
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| state.state)
            .collect();

        assert_eq!(states, [State::Ok]);
    }

    /// list-2: a recorded jar whose name does not end with `.jar` is held against its record like
    /// any other file.
    #[test]
    fn recorded_plugin_without_the_jar_suffix_is_ok() {
        let fixture = Fixture::with_plugin_records(vec![record(
            "Example",
            "Example.jar.disabled",
            &sha256_hex(b"jar"),
        )]);
        fixture.put_in_plugins("Example.jar.disabled", b"jar");

        let states: Vec<_> = fixture
            .plugins()
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| state.state)
            .collect();

        assert_eq!(states, [State::Ok]);
    }

    /// drm-1: a recorded datapack file whose name does not end with `.zip` is removed with its
    /// record.
    #[test]
    fn removal_of_a_datapack_without_the_zip_suffix_removes_its_file() {
        let removed_record = record("Pack", "Pack.mcpack", &"2".repeat(64));
        let fixture = Fixture::with_records(Vec::new(), vec![removed_record.clone()]);
        fixture.put_in(&fixture.world_datapacks(), "Pack.mcpack", b"zip");

        let removed = fixture.world_datapacks().remove("Pack").unwrap();

        assert_eq!(
            (
                removed,
                fixture
                    .world_datapacks()
                    .directory
                    .join("Pack.mcpack")
                    .exists()
            ),
            (Removed::FileAndRecord(removed_record), false)
        );
    }

    /// dlist-1, dlist-2: when the world directory's path is a regular file, the listing succeeds
    /// and the record is `missing`.
    #[test]
    fn world_that_is_a_regular_file_lists_its_records_as_missing() {
        let fixture = Fixture::with_records(
            Vec::new(),
            vec![record("Pack", "Pack.zip", &sha256_hex(b"zip"))],
        );
        fs::write(fixture.server.path.join("world"), b"not a directory").unwrap();

        let states: Vec<_> = fixture
            .world_datapacks()
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| state.state)
            .collect();

        assert_eq!(states, [State::Missing]);
    }

    /// list-1, list-2: when `plugins` is a regular file, the listing succeeds and the record is
    /// `missing`.
    #[test]
    fn plugins_that_is_a_regular_file_lists_its_records_as_missing() {
        let fixture =
            Fixture::with_plugin_records(vec![record("Example", "Example.jar", &"1".repeat(64))]);
        fs::write(fixture.server.path.join("plugins"), b"not a directory").unwrap();

        let states: Vec<_> = fixture
            .plugins()
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| state.state)
            .collect();

        assert_eq!(states, [State::Missing]);
    }

    /// drm-3: when the world directory's path is a regular file, only the record is removed.
    #[test]
    fn removal_with_a_world_that_is_a_regular_file_removes_only_the_record() {
        let removed_record = record("Pack", "Pack.zip", &"2".repeat(64));
        let fixture = Fixture::with_records(Vec::new(), vec![removed_record.clone()]);
        fs::write(fixture.server.path.join("world"), b"not a directory").unwrap();

        let removed = fixture.world_datapacks().remove("Pack").unwrap();

        assert_eq!(
            (removed, fixture.read_mism_toml().datapacks),
            (Removed::RecordOnly(removed_record), Vec::new())
        );
    }

    /// 「プラグインとデータパックを置くディレクトリ」 方法: a dangling symbolic link where the world
    /// directory would be prevents placing.
    #[test]
    fn dangling_link_in_the_world_directory_path_prevents_placing() {
        let fixture = Fixture::new();
        symlink(
            fixture.server.path.join("nowhere"),
            fixture.server.path.join("world"),
        )
        .unwrap();

        let error = fixture
            .world_datapacks()
            .check_placeable("Pack", "Pack.zip")
            .unwrap_err();

        assert!(
            matches!(&error, Error::NotDirectory { path } if *path == fixture.server.path.join("world")),
            "{error}"
        );
    }
}
