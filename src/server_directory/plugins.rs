use std::fmt;
use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::ServerDirectory;
use super::mism_toml::{self, MismToml, PluginRecord};
use crate::atomic_fs::{self, UnnamedFile};
use crate::content_hash::{self, HashingWriter};
use crate::file_name;
use crate::plugin_jar::{self, Declaration};

const JAR_SUFFIX: &str = ".jar";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to list {}", path.display())]
    ListPlugins {
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
    JarFileNameNotUtf8 { path: PathBuf },
    #[error("failed to read the declaration of {}", path.display())]
    UnreadableDeclaration {
        path: PathBuf,
        #[source]
        source: plugin_jar::Error,
    },
    #[error("mism.toml already has a record of the plugin {name}")]
    RecordExists { name: String },
    #[error("{} already exists", path.display())]
    JarExists { path: PathBuf },
    #[error("{} is not a directory", path.display())]
    PluginsNotDirectory { path: PathBuf },
    #[error("failed to create {}", path.display())]
    CreatePlugins {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to copy {} into {}", from.display(), to.display())]
    CopyJar {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "the SHA-256 of {} is {actual}, not the name of its directory in the cache directory",
        path.display()
    )]
    CachedJarSha256Mismatch { path: PathBuf, actual: String },
    #[error(transparent)]
    AtomicFs(#[from] atomic_fs::Error),
    #[error(transparent)]
    MismToml(#[from] mism_toml::Error),
    #[error("mism.toml has no record of the plugin {name}")]
    RecordNotFound { name: String },
    #[error("mism.toml has several records of the plugin {name}")]
    SeveralRecords { name: String },
    #[error("the file name in the record of {name} cannot be used")]
    RecordedFileName {
        name: String,
        #[source]
        source: file_name::Error,
    },
    #[error(transparent)]
    HashJar(content_hash::Error),
    #[error("failed to remove {}, to which the removed plugin's jar was renamed", path.display())]
    RemoveRenamedJar {
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
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::UnreadableDeclaration { path, .. } => {
                Some(format!("move {} out of plugins/", path.display()))
            }
            Error::RecordExists { name } => Some(format!(
                "run `mism plugin remove {name}` to remove the record first"
            )),
            Error::JarExists { path } => Some(format!("move {} out of plugins/", path.display())),
            Error::CachedJarSha256Mismatch { path, .. } => Some(format!(
                "remove the directory {} and fetch the plugin again",
                path.parent()
                    .expect("a cached jar is in a SHA-256 directory")
                    .display()
            )),
            Error::RecordNotFound { .. } => {
                Some("run `mism plugin list` to see the names in the records".to_owned())
            }
            Error::AtomicFs(error) => error.hint(),
            Error::MismToml(error) => error.hint(),
            Error::HashJar(error) => error.hint(),
            Error::Undo { failure, .. } => failure.hint(),
            Error::ListPlugins { .. }
            | Error::Examine { .. }
            | Error::JarFileNameNotUtf8 { .. }
            | Error::PluginsNotDirectory { .. }
            | Error::CreatePlugins { .. }
            | Error::CopyJar { .. }
            | Error::SeveralRecords { .. }
            | Error::RecordedFileName { .. }
            | Error::RemoveRenamedJar { .. } => None,
        }
    }
}

/// A jar directly in `plugins/` and its declaration, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerJar {
    pub file_name: String,
    pub declaration: Option<Declaration>,
}

/// A jar in the cache directory to place, and its record (plg-4).
#[derive(Debug, Clone, Copy)]
pub struct JarToPlace<'a> {
    /// The jar in its SHA-256 directory of the cache directory.
    pub cached_path: &'a Path,
    /// The name of that SHA-256 directory.
    pub sha256: &'a str,
    pub plugin_name: &'a str,
    pub source: &'a str,
    pub file_name: &'a str,
}

/// What `remove` removed (rm-1, rm-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// The jar in `plugins/` and its record.
    JarAndRecord(PluginRecord),
    /// Only the record: `plugins/` had no jar of that name.
    RecordOnly(PluginRecord),
}

/// The state of a plugin in `mism plugin list` (list-2).
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

/// A record or an unrecorded jar, held against each other (list-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginState {
    pub state: State,
    /// The record's name; for `unmanaged`, the file name without `.jar`.
    pub name: String,
    pub file_name: String,
    /// The record's source; `None` for `unmanaged`.
    pub source: Option<String>,
}

/// Each jar directly in `plugins/` by file name, with its declaration; none without `plugins/`.
pub fn jars(server: &ServerDirectory) -> Result<Vec<ServerJar>, Error> {
    let plugins = server.plugins_path();
    jar_file_names(&plugins)?
        .into_iter()
        .map(|file_name| {
            let path = plugins.join(&file_name);
            let declaration = plugin_jar::read_declaration(&path)
                .map_err(|source| Error::UnreadableDeclaration { path, source })?;
            Ok(ServerJar {
                file_name,
                declaration,
            })
        })
        .collect()
}

/// The names of the jars directly in `plugins`, in ascending order: names ending with `.jar` of
/// what is a regular file after following symbolic links.
fn jar_file_names(plugins: &Path) -> Result<Vec<String>, Error> {
    let list_failed = |source| Error::ListPlugins {
        path: plugins.to_path_buf(),
        source,
    };
    let entries = match fs::read_dir(plugins) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(list_failed(error)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let path = entry.map_err(list_failed)?.path();
        let ends_with_jar = path
            .file_name()
            .is_some_and(|name| name.as_bytes().ends_with(JAR_SUFFIX.as_bytes()));
        if ends_with_jar && is_regular_file(&path)? {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::JarFileNameNotUtf8 { path: path.clone() })?;
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Follows symbolic links; nothing at the path, or a dangling link, is no regular file.
fn is_regular_file(path: &Path) -> Result<bool, Error> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(Error::Examine {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Whether `plugins/<file_name>` is a jar directly in `plugins/`.
fn is_jar(plugins: &Path, file_name: &str) -> Result<bool, Error> {
    Ok(file_name.ends_with(JAR_SUFFIX) && is_regular_file(&plugins.join(file_name))?)
}

/// The conditions to check before placing (place-3, place-4).
pub fn check_placeable(
    server: &ServerDirectory,
    plugin_name: &str,
    file_name: &str,
) -> Result<(), Error> {
    if server
        .mism_toml
        .plugins
        .iter()
        .any(|record| record.name == plugin_name)
    {
        return Err(Error::RecordExists {
            name: plugin_name.to_owned(),
        });
    }
    let plugins = server.plugins_path();
    if !plugins_is_directory_or_missing(&plugins)? {
        return Err(Error::PluginsNotDirectory { path: plugins });
    }
    let path = plugins.join(file_name);
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(Error::JarExists { path }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Examine { path, source }),
    }
}

fn plugins_is_directory_or_missing(plugins: &Path) -> Result<bool, Error> {
    match fs::metadata(plugins) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(source) => Err(Error::Examine {
            path: plugins.to_path_buf(),
            source,
        }),
    }
}

/// Places the jar in `plugins/` and adds its record to `mism.toml` (place-2). On a failure,
/// `plugins/` and `mism.toml` are as before (plg-8).
pub fn place(server: &ServerDirectory, jar: &JarToPlace) -> Result<(), Error> {
    let plugins = server.plugins_path();
    let created = create_plugins_if_missing(&plugins)?;
    place_in(server, &plugins, jar).map_err(|failure| {
        if created {
            undo(failure, &plugins, fs::remove_dir(&plugins))
        } else {
            failure
        }
    })
}

/// Step 1: whether `plugins/` was created.
fn create_plugins_if_missing(plugins: &Path) -> Result<bool, Error> {
    match fs::create_dir(plugins) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(source) => Err(Error::CreatePlugins {
            path: plugins.to_path_buf(),
            source,
        }),
    }
}

/// Steps 2 to 4, undoing step 3 when step 4 fails.
fn place_in(server: &ServerDirectory, plugins: &Path, jar: &JarToPlace) -> Result<(), Error> {
    let file = copy_verified(plugins, jar)?;
    let placed = plugins.join(jar.file_name);
    link_jar(file, &placed)?;
    add_record(server, jar).map_err(|failure| undo(failure, &placed, fs::remove_file(&placed)))
}

/// Step 2 (d-pdir-1): the copy is checked against the name of the cached jar's SHA-256
/// directory.
fn copy_verified(plugins: &Path, jar: &JarToPlace) -> Result<UnnamedFile, Error> {
    let copy_failed = |source| Error::CopyJar {
        from: jar.cached_path.to_path_buf(),
        to: plugins.to_path_buf(),
        source,
    };
    let mut hashing = HashingWriter::new(UnnamedFile::create_in(plugins)?);
    io::copy(
        &mut File::open(jar.cached_path).map_err(copy_failed)?,
        &mut hashing,
    )
    .map_err(copy_failed)?;
    let hashed = hashing.finish();
    if hashed.sha256 == jar.sha256 {
        Ok(hashed.inner)
    } else {
        Err(Error::CachedJarSha256Mismatch {
            path: jar.cached_path.to_path_buf(),
            actual: hashed.sha256,
        })
    }
}

/// Step 3: a name that another process created after the check is place-4's error.
fn link_jar(file: UnnamedFile, placed: &Path) -> Result<(), Error> {
    file.link_as(placed).map_err(|error| match error {
        atomic_fs::Error::AlreadyExists { path } => Error::JarExists { path },
        error => Error::AtomicFs(error),
    })
}

/// Step 4.
fn add_record(server: &ServerDirectory, jar: &JarToPlace) -> Result<(), Error> {
    let mut plugins = server.mism_toml.plugins.clone();
    plugins.push(PluginRecord {
        name: jar.plugin_name.to_owned(),
        source: jar.source.to_owned(),
        file_name: jar.file_name.to_owned(),
        sha256: jar.sha256.to_owned(),
    });
    write_mism_toml(server, plugins)
}

fn write_mism_toml(server: &ServerDirectory, plugins: Vec<PluginRecord>) -> Result<(), Error> {
    let mism_toml = MismToml {
        plugins,
        ..server.mism_toml.clone()
    };
    Ok(mism_toml.write(&server.mism_toml_path())?)
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

/// Removes the jar of the record named `name` and the record (rm-1, rm-2, rm-3). The jar is
/// renamed before `mism.toml` changes and deleted after, so that a failure to change either
/// leaves both as before (plg-8).
pub fn remove(server: &ServerDirectory, name: &str) -> Result<Removed, Error> {
    let (index, record) = find_record(server, name)?;
    let plugins = server.plugins_path();
    let renamed = rename_jar_away(&plugins, &record.file_name)?;
    let mut records = server.mism_toml.plugins.clone();
    records.remove(index);
    write_mism_toml(server, records).map_err(|failure| match &renamed {
        Some(working_path) => undo(
            failure,
            working_path,
            atomic_fs::rename_without_replace(working_path, &plugins.join(&record.file_name)),
        ),
        None => failure,
    })?;
    match renamed {
        Some(working_path) => {
            delete_renamed_jar(&working_path)?;
            Ok(Removed::JarAndRecord(record))
        }
        None => Ok(Removed::RecordOnly(record)),
    }
}

/// Step 1: the only record named `name`, whose file name is checked before use.
fn find_record(server: &ServerDirectory, name: &str) -> Result<(usize, PluginRecord), Error> {
    let mut found = server
        .mism_toml
        .plugins
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

fn check_recorded_file_name(record: &PluginRecord) -> Result<(), Error> {
    file_name::check_jar_file_name(&record.file_name).map_err(|source| Error::RecordedFileName {
        name: record.name.clone(),
        source,
    })
}

/// Step 2: the working name the jar now has, or `None` when there is no such jar (rm-3).
fn rename_jar_away(plugins: &Path, file_name: &str) -> Result<Option<PathBuf>, Error> {
    if !is_jar(plugins, file_name)? {
        return Ok(None);
    }
    let working_path = plugins.join(atomic_fs::working_name()?);
    atomic_fs::rename_without_replace(&plugins.join(file_name), &working_path)?;
    Ok(Some(working_path))
}

/// Step 4.
fn delete_renamed_jar(working_path: &Path) -> Result<(), Error> {
    fs::remove_file(working_path).map_err(|source| Error::RemoveRenamedJar {
        path: working_path.to_path_buf(),
        source,
    })
}

/// The records in their order, then the unrecorded jars by name (list-1, list-2).
pub fn reconcile(server: &ServerDirectory) -> Result<Vec<PluginState>, Error> {
    let plugins = server.plugins_path();
    let mut states = Vec::new();
    for record in &server.mism_toml.plugins {
        states.push(PluginState {
            state: record_state(&plugins, record)?,
            name: record.name.clone(),
            file_name: record.file_name.clone(),
            source: Some(record.source.clone()),
        });
    }
    for file_name in jar_file_names(&plugins)? {
        let recorded = server
            .mism_toml
            .plugins
            .iter()
            .any(|record| record.file_name == file_name);
        if !recorded {
            states.push(PluginState {
                state: State::Unmanaged,
                name: file_name
                    .strip_suffix(JAR_SUFFIX)
                    .expect("a jar's name ends with .jar")
                    .to_owned(),
                file_name,
                source: None,
            });
        }
    }
    Ok(states)
}

fn record_state(plugins: &Path, record: &PluginRecord) -> Result<State, Error> {
    check_recorded_file_name(record)?;
    if !is_jar(plugins, &record.file_name)? {
        return Ok(State::Missing);
    }
    let sha256 =
        content_hash::sha256_of_file(&plugins.join(&record.file_name)).map_err(Error::HashJar)?;
    Ok(if sha256 == record.sha256 {
        State::Ok
    } else {
        State::Changed
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{Cursor, Write};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

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

    /// A jar whose `plugin.yml` is `plugin_yml`.
    fn jar_bytes(plugin_yml: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("plugin.yml", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(plugin_yml.as_bytes()).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn record(name: &str, file_name: &str, sha256: &str) -> PluginRecord {
        PluginRecord {
            name: name.to_owned(),
            source: format!("https://example.com/{file_name}"),
            file_name: file_name.to_owned(),
            sha256: sha256.to_owned(),
        }
    }

    /// A server directory with its `mism.toml` written, a cached jar outside it, and the
    /// temporary directories holding both.
    struct Fixture {
        server: ServerDirectory,
        _server_root: TempDir,
        cache: TempDir,
    }

    impl Fixture {
        fn with_records(records: Vec<PluginRecord>) -> Fixture {
            let server_root = temporary_directory();
            let server = ServerDirectory {
                path: server_root.path().to_path_buf(),
                mism_toml: MismToml {
                    software: "paper".to_owned(),
                    minecraft_version: "26.3".to_owned(),
                    software_version: 206,
                    plugins: records,
                },
            };
            server.mism_toml.write(&server.mism_toml_path()).unwrap();
            Fixture {
                server,
                _server_root: server_root,
                cache: temporary_directory(),
            }
        }

        fn new() -> Fixture {
            Fixture::with_records(Vec::new())
        }

        fn plugins(&self) -> PathBuf {
            self.server.plugins_path()
        }

        fn put_in_plugins(&self, file_name: &str, content: &[u8]) {
            fs::create_dir_all(self.plugins()).unwrap();
            fs::write(self.plugins().join(file_name), content).unwrap();
        }

        /// Writes `content` as a cached jar in a directory named `sha256`.
        fn cached_jar(&self, sha256: &str, content: &[u8]) -> PathBuf {
            let directory = self.cache.path().join(sha256);
            fs::create_dir(&directory).unwrap();
            let path = directory.join("Example.jar");
            fs::write(&path, content).unwrap();
            path
        }

        fn place(&self, cached_path: &Path, sha256: &str) -> Result<(), Error> {
            place(
                &self.server,
                &JarToPlace {
                    cached_path,
                    sha256,
                    plugin_name: "Example",
                    source: "https://example.com/Example.jar",
                    file_name: "Example.jar",
                },
            )
        }

        fn read_mism_toml(&self) -> MismToml {
            MismToml::read(&self.server.mism_toml_path()).unwrap()
        }

        /// Every file and directory in the server directory with its content.
        fn snapshot(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
            let mut entries = BTreeMap::new();
            collect(&self.server.path, &self.server.path, &mut entries);
            entries
        }
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
            fs::read(fixture.plugins().join("Example.jar")).unwrap(),
            content
        );
    }

    /// place-2, plg-4: the record is added at the end of `mism.toml`.
    #[test]
    fn placed_jar_is_recorded_last() {
        let existing = record("Vault", "Vault.jar", &"1".repeat(64));
        let fixture = Fixture::with_records(vec![existing.clone()]);
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

        assert!(matches!(result, Err(Error::CachedJarSha256Mismatch { .. })));
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
        fs::create_dir(fixture.plugins()).unwrap();
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

        assert!(matches!(result, Err(Error::JarExists { .. })));
    }

    /// place-3
    #[test]
    fn record_with_the_same_name_prevents_placing() {
        let fixture = Fixture::with_records(vec![record("Example", "Other.jar", &"1".repeat(64))]);

        assert!(matches!(
            check_placeable(&fixture.server, "Example", "Example.jar"),
            Err(Error::RecordExists { .. })
        ));
    }

    /// place-4: anything named like the jar in `plugins/`, a dangling link too, prevents placing.
    #[test]
    fn dangling_link_with_the_jar_name_prevents_placing() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.plugins()).unwrap();
        symlink("missing", fixture.plugins().join("Example.jar")).unwrap();

        assert!(matches!(
            check_placeable(&fixture.server, "Example", "Example.jar"),
            Err(Error::JarExists { .. })
        ));
    }

    /// 「サーバーのディレクトリのプラグイン」 方法: a `plugins` that is not a directory prevents
    /// placing.
    #[test]
    fn plugins_that_is_a_file_prevents_placing() {
        let fixture = Fixture::new();
        fs::write(fixture.plugins(), b"not a directory").unwrap();

        assert!(matches!(
            check_placeable(&fixture.server, "Example", "Example.jar"),
            Err(Error::PluginsNotDirectory { .. })
        ));
    }

    /// place-3, place-4: a new name and jar can be placed.
    #[test]
    fn new_name_and_jar_can_be_placed() {
        let fixture = Fixture::with_records(vec![record("Vault", "Vault.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Vault.jar", b"vault");

        assert!(check_placeable(&fixture.server, "Example", "Example.jar").is_ok());
    }

    /// rm-1: the jar and the record are removed, other files in `plugins/` stay.
    #[test]
    fn removal_removes_the_jar_and_the_record_only() {
        let fixture =
            Fixture::with_records(vec![record("Example", "Example.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Example.jar", b"jar");
        fixture.put_in_plugins("config.yml", b"kept");

        remove(&fixture.server, "Example").unwrap();

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
        let fixture = Fixture::with_records(vec![removed_record.clone()]);
        fixture.put_in_plugins("Example.jar", b"jar");

        assert_eq!(
            remove(&fixture.server, "Example").unwrap(),
            Removed::JarAndRecord(removed_record)
        );
    }

    /// rm-2
    #[test]
    fn removal_of_an_unrecorded_name_is_an_error_and_changes_nothing() {
        let fixture =
            Fixture::with_records(vec![record("Example", "Example.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Example.jar", b"jar");
        let before = fixture.snapshot();

        let result = remove(&fixture.server, "Other");

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
        let fixture = Fixture::with_records(vec![removed_record.clone()]);

        let removed = remove(&fixture.server, "Example").unwrap();

        assert_eq!(
            (removed, fixture.read_mism_toml().plugins),
            (Removed::RecordOnly(removed_record), Vec::new())
        );
    }

    /// d-name-1
    #[test]
    fn record_file_name_outside_plugins_is_an_error_and_removes_nothing() {
        let fixture = Fixture::with_records(vec![record(
            "Example",
            "../server.properties",
            &"1".repeat(64),
        )]);
        fs::write(fixture.server.path.join("server.properties"), b"motd=A").unwrap();
        let before = fixture.snapshot();

        let result = remove(&fixture.server, "Example");

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
            Fixture::with_records(vec![record("Example", "Example.jar", &"1".repeat(64))]);
        fixture.put_in_plugins("Example.jar", b"jar");
        let before = fixture.snapshot();

        let result = {
            let _read_only = ReadOnly::new(&fixture.server.path);
            remove(&fixture.server, "Example")
        };

        assert_eq!((result.is_err(), fixture.snapshot()), (true, before));
    }

    /// d-pdir-2, list-2: the records in their order, then the unrecorded jars by name.
    #[test]
    fn reconciliation_lists_records_then_unrecorded_jars() {
        let fixture = Fixture::with_records(vec![
            record("Zeta", "Zeta.jar", &sha256_hex(b"zeta")),
            record("Alpha", "Alpha.jar", &sha256_hex(b"alpha")),
            record("Gone", "Gone.jar", &"1".repeat(64)),
        ]);
        fixture.put_in_plugins("Zeta.jar", b"zeta");
        fixture.put_in_plugins("Alpha.jar", b"changed");
        fixture.put_in_plugins("b-extra.jar", b"b");
        fixture.put_in_plugins("a-extra.jar", b"a");
        fixture.put_in_plugins("notes.txt", b"not a jar");
        fs::create_dir(fixture.plugins().join("Directory.jar")).unwrap();

        let states: Vec<_> = reconcile(&fixture.server)
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

    /// 「サーバーのディレクトリのプラグイン」 方法: the jars in `plugins/` with their
    /// declarations; a jar without one has none.
    #[test]
    fn jars_in_plugins_have_their_declarations() {
        let fixture = Fixture::new();
        fixture.put_in_plugins("Vault.jar", &jar_bytes("name: Vault\n"));
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("README", SimpleFileOptions::default())
            .unwrap();
        fixture.put_in_plugins("Nameless.jar", &writer.finish().unwrap().into_inner());

        let names: Vec<_> = jars(&fixture.server)
            .unwrap()
            .into_iter()
            .map(|jar| {
                (
                    jar.file_name,
                    jar.declaration.and_then(|declaration| declaration.name),
                )
            })
            .collect();

        assert_eq!(
            names,
            [
                ("Nameless.jar".to_owned(), None),
                ("Vault.jar".to_owned(), Some("Vault".to_owned()))
            ]
        );
    }

    /// 「サーバーのディレクトリのプラグイン」 方法: a jar whose declaration cannot be read is an
    /// error whose hint is to move it out.
    #[test]
    fn jar_with_an_unreadable_declaration_is_an_error() {
        let fixture = Fixture::new();
        fixture.put_in_plugins("Broken.jar", b"not a zip file");

        let error = jars(&fixture.server).unwrap_err();

        assert_eq!(
            error.hint(),
            Some(format!(
                "move {} out of plugins/",
                fixture.plugins().join("Broken.jar").display()
            ))
        );
    }

    /// 「サーバーのディレクトリのプラグイン」 方法: without `plugins/` there are no jars.
    #[test]
    fn server_without_plugins_has_no_jars() {
        let fixture = Fixture::new();

        assert_eq!(jars(&fixture.server).unwrap(), []);
    }
}
