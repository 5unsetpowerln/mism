mod fetch;
mod inspect;
mod install;
mod list;
mod place;
mod remove;
mod search;
mod versions;

use std::io;
use std::path::{Path, PathBuf};

use crate::command_line::{PluginArguments, PluginSubcommand};
use crate::fetch_source::{self, FetchSource};
use crate::fetched_file_cache::{self, CachedFile, FetchedFileCache};
use crate::file_name;
use crate::interruption::Signal;
use crate::modrinth_api;
use crate::modrinth_source;
use crate::output;
use crate::plugin_compatibility::TargetServer;
use crate::server_directory::ServerDirectory;
use crate::server_directory::placed_files;

/// The end of a jar's name (plg-3, plg-5).
const JAR_SUFFIX: &str = ".jar";
/// The directory of the cache directory that holds the fetched jars.
const CACHE_DIRECTORY_NAME: &str = "plugins";
const THREE_FORMS: &str = "give `modrinth:<identifier>` (optionally followed by `:<value>`s), a \
                           jar URL starting with `https://` or `http://`, or the path of a local \
                           jar file";
const SEARCH_HINT: &str = "run `mism plugin search <term>` to find the identifier";

/// What a `mism plugin` subcommand reports. The subcommands' errors are boxed: they are large,
/// and every result up to `main` would carry their size.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Search(#[from] Box<search::Error>),
    #[error(transparent)]
    Versions(#[from] Box<versions::Error>),
    #[error(transparent)]
    Fetch(#[from] Box<fetch::Error>),
    #[error(transparent)]
    Inspect(#[from] Box<inspect::Error>),
    #[error(transparent)]
    Place(#[from] Box<place::Error>),
    #[error(transparent)]
    Install(#[from] Box<install::Error>),
    #[error(transparent)]
    List(#[from] Box<list::Error>),
    #[error(transparent)]
    Remove(#[from] Box<remove::Error>),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Search(error) => error.hint(),
            Error::Versions(error) => error.hint(),
            Error::Fetch(error) => error.hint(),
            Error::Inspect(error) => error.hint(),
            Error::Place(error) => error.hint(),
            Error::Install(error) => error.hint(),
            Error::List(error) => error.hint(),
            Error::Remove(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Search(error) => error.interrupted_by(),
            Error::Versions(error) => error.interrupted_by(),
            Error::Fetch(error) => error.interrupted_by(),
            Error::Inspect(error) => error.interrupted_by(),
            Error::Place(error) => error.interrupted_by(),
            Error::Install(error) => error.interrupted_by(),
            Error::List(error) => error.interrupted_by(),
            Error::Remove(error) => error.interrupted_by(),
        }
    }
}

pub fn run(arguments: &PluginArguments) -> Result<(), Error> {
    match &arguments.subcommand {
        PluginSubcommand::Search(arguments) => Ok(search::run(arguments).map_err(Box::new)?),
        PluginSubcommand::Versions(arguments) => Ok(versions::run(arguments).map_err(Box::new)?),
        PluginSubcommand::Fetch(arguments) => Ok(fetch::run(arguments).map_err(Box::new)?),
        PluginSubcommand::Inspect(arguments) => Ok(inspect::run(arguments).map_err(Box::new)?),
        PluginSubcommand::Place(arguments) => Ok(place::run(arguments).map_err(Box::new)?),
        PluginSubcommand::Install(arguments) => Ok(install::run(arguments).map_err(Box::new)?),
        PluginSubcommand::List(arguments) => Ok(list::run(arguments).map_err(Box::new)?),
        PluginSubcommand::Remove(arguments) => Ok(remove::run(arguments).map_err(Box::new)?),
    }
}

/// The server software and the Minecraft version of the target server, to hold plugins against
/// (vers-3, insp-3, inst-2).
fn target_of(server: &ServerDirectory) -> TargetServer<'_> {
    TargetServer {
        software: &server.mism_toml.software,
        minecraft_version: &server.mism_toml.minecraft_version,
    }
}

/// A plugin's <取得元の指定> (plg-3).
fn parse_source(text: &str) -> Result<FetchSource, SourceError> {
    Ok(fetch_source::parse(text, JAR_SUFFIX)?)
}

/// The jars fetched into the cache directory.
fn open_cache() -> Result<FetchedFileCache, CacheError> {
    Ok(FetchedFileCache::open(CACHE_DIRECTORY_NAME)?)
}

/// The jar fetched with the source `text`, or for a local jar the cached jar with its content
/// (place-1, insp-1).
fn find_in_cache(text: &str, source: &FetchSource) -> Result<Option<CachedFile>, CacheError> {
    Ok(open_cache()?.find(text, source.local_path())?)
}

/// The hint for a failed request to Modrinth: for an identifier of no project, the search
/// (plg-3).
fn modrinth_hint(error: &modrinth_api::Error) -> Option<String> {
    match error {
        modrinth_api::Error::ProjectNotFound { .. } => Some(SEARCH_HINT.to_owned()),
        error => error.hint(),
    }
}

/// Written after the result of a subcommand that changed `plugins/` (plg-7).
fn write_takes_effect_notice() -> Result<(), output::Error> {
    output::write_to_stdout("the change takes effect when the server starts next")
}

/// The hint for a cached file that cannot be used: to remove its SHA-256 directory.
fn refetch_hint(path: &Path) -> String {
    format!(
        "remove the directory {} and fetch the plugin again",
        path.parent()
            .expect("a file of the cache directory is in a SHA-256 directory")
            .display()
    )
}

/// A plugin name or a jar file name that cannot be used (plg-6).
#[derive(Debug, thiserror::Error)]
pub enum NameError {
    #[error("the plugin name {:?} cannot be used: {}", .0.name(), .0.reason())]
    PluginName(file_name::Error),
    #[error("the jar file name {:?} cannot be used: {}", .0.name(), .0.reason())]
    JarFileName(file_name::Error),
}

impl From<file_name::Error> for NameError {
    fn from(error: file_name::Error) -> NameError {
        match error {
            file_name::Error::Name { .. } => NameError::PluginName(error),
            file_name::Error::FileName { .. } => NameError::JarFileName(error),
        }
    }
}

/// A plugin's source that is none of its three forms (plg-3).
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("{} is not a source: failed to examine it as a local jar file", path.display())]
    ExamineLocalJar {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Name(NameError),
    #[error(transparent)]
    Source(fetch_source::Error),
}

impl From<fetch_source::Error> for SourceError {
    fn from(error: fetch_source::Error) -> SourceError {
        match error {
            fetch_source::Error::ExamineLocalFile { path, source } => {
                SourceError::ExamineLocalJar { path, source }
            }
            fetch_source::Error::FileName(error) => SourceError::Name(error.into()),
            error => SourceError::Source(error),
        }
    }
}

impl From<modrinth_source::Error> for SourceError {
    fn from(error: modrinth_source::Error) -> SourceError {
        SourceError::Source(error.into())
    }
}

impl SourceError {
    pub fn hint(&self) -> Option<String> {
        match self {
            SourceError::ExamineLocalJar { .. }
            | SourceError::Source(
                fetch_source::Error::NotRegularFile { .. }
                | fetch_source::Error::WrongSuffix { .. },
            ) => Some(THREE_FORMS.to_owned()),
            SourceError::Source(fetch_source::Error::Modrinth(
                modrinth_source::Error::InvalidIdentifier { .. },
            )) => Some(SEARCH_HINT.to_owned()),
            SourceError::Source(error) => error.hint(),
            SourceError::Name(_) => None,
        }
    }
}

/// A failure of the jars in the cache directory.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error(
        "several directories of the cache directory hold the jar with the SHA-256 {sha256}: {}",
        fetched_file_cache::list_paths(.paths)
    )]
    DuplicateSha256 { sha256: String, paths: Vec<PathBuf> },
    #[error(
        "the cache directory has several jars fetched from {fetched_from}: {}",
        fetched_file_cache::list_paths(.paths)
    )]
    SeveralJars {
        fetched_from: String,
        paths: Vec<PathBuf>,
    },
    #[error(transparent)]
    Name(NameError),
    #[error("the file name recorded in {} cannot be used", path.display())]
    RecordedFileName {
        path: PathBuf,
        #[source]
        source: NameError,
    },
    #[error(transparent)]
    Cache(fetched_file_cache::Error),
}

impl From<fetched_file_cache::Error> for CacheError {
    fn from(error: fetched_file_cache::Error) -> CacheError {
        match error {
            fetched_file_cache::Error::DuplicateSha256 { sha256, paths } => {
                CacheError::DuplicateSha256 { sha256, paths }
            }
            fetched_file_cache::Error::SeveralFiles {
                fetched_from,
                paths,
            } => CacheError::SeveralJars {
                fetched_from,
                paths,
            },
            fetched_file_cache::Error::FileName(error) => CacheError::Name(error.into()),
            fetched_file_cache::Error::RecordedFileName { path, source } => {
                CacheError::RecordedFileName {
                    path,
                    source: source.into(),
                }
            }
            error => CacheError::Cache(error),
        }
    }
}

impl CacheError {
    pub fn hint(&self) -> Option<String> {
        match self {
            CacheError::SeveralJars { .. } => Some(
                "give the path of one of these jars in the cache directory as the source"
                    .to_owned(),
            ),
            CacheError::RecordedFileName { path, .. }
            | CacheError::Cache(
                fetched_file_cache::Error::ReadSourceToml { path, .. }
                | fetched_file_cache::Error::ParseSourceToml { path, .. },
            ) => Some(refetch_hint(path)),
            CacheError::Cache(error) => error.hint(),
            CacheError::DuplicateSha256 { .. } | CacheError::Name(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            CacheError::Cache(error) => error.interrupted_by(),
            CacheError::DuplicateSha256 { .. }
            | CacheError::SeveralJars { .. }
            | CacheError::Name(_)
            | CacheError::RecordedFileName { .. } => None,
        }
    }
}

/// A failure of the jars in `plugins/` and their records.
#[derive(Debug, thiserror::Error)]
pub enum PlacedFilesError {
    #[error("mism.toml already has a record of the plugin {name}")]
    RecordExists { name: String },
    #[error("mism.toml has no record of the plugin {name}")]
    RecordNotFound { name: String },
    #[error("mism.toml has several records of the plugin {name}")]
    SeveralRecords { name: String },
    #[error("the file name in the record of {name} cannot be used")]
    RecordedFileName {
        name: String,
        #[source]
        source: NameError,
    },
    #[error("failed to remove {}, to which the removed plugin's jar was renamed", path.display())]
    RemoveRenamedJar {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    PlacedFiles(placed_files::Error),
}

impl From<placed_files::Error> for PlacedFilesError {
    fn from(error: placed_files::Error) -> PlacedFilesError {
        match error {
            placed_files::Error::RecordExists { name } => PlacedFilesError::RecordExists { name },
            placed_files::Error::RecordNotFound { name } => {
                PlacedFilesError::RecordNotFound { name }
            }
            placed_files::Error::SeveralRecords { name } => {
                PlacedFilesError::SeveralRecords { name }
            }
            placed_files::Error::RecordedFileName { name, source } => {
                PlacedFilesError::RecordedFileName {
                    name,
                    source: source.into(),
                }
            }
            placed_files::Error::RemoveRenamedFile { path, source } => {
                PlacedFilesError::RemoveRenamedJar { path, source }
            }
            error => PlacedFilesError::PlacedFiles(error),
        }
    }
}

impl PlacedFilesError {
    pub fn hint(&self) -> Option<String> {
        match self {
            PlacedFilesError::RecordExists { name } => Some(format!(
                "run `mism plugin remove {name}` to remove the record first"
            )),
            PlacedFilesError::RecordNotFound { .. } => {
                Some("run `mism plugin list` to see the names in the records".to_owned())
            }
            PlacedFilesError::PlacedFiles(error) => match error.failure() {
                placed_files::Error::FileExists { path } => {
                    Some(format!("move {} out of plugins/", path.display()))
                }
                placed_files::Error::CachedFileSha256Mismatch { path, .. } => {
                    Some(refetch_hint(path))
                }
                _ => error.hint(),
            },
            PlacedFilesError::SeveralRecords { .. }
            | PlacedFilesError::RecordedFileName { .. }
            | PlacedFilesError::RemoveRenamedJar { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::http_client;

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    /// plg-3: a string that is none of the three forms is an error with the forms as the hint.
    #[test]
    fn missing_local_file_is_an_error_with_the_three_forms_as_hint() {
        let directory = temporary_directory();
        let missing = directory.path().join("missing.jar");

        let error = parse_source(missing.to_str().unwrap()).unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some(
                "give `modrinth:<identifier>` (optionally followed by `:<value>`s), a jar URL \
                 starting with `https://` or `http://`, or the path of a local jar file"
            )
        );
    }

    /// d-source-3: the identifier is checked, with a hint at the plugin search.
    #[test]
    fn invalid_identifier_hints_at_the_plugin_search() {
        assert_eq!(
            parse_source("modrinth:../search")
                .unwrap_err()
                .hint()
                .as_deref(),
            Some("run `mism plugin search <term>` to find the identifier")
        );
    }

    /// 「Modrinth の API」 方法: a project that does not exist is reported with a hint to search.
    #[test]
    fn missing_project_hints_at_searching() {
        let error = modrinth_api::Error::ProjectNotFound {
            identifier: "nosuchplugin".to_owned(),
            source: http_client::Error::Status {
                url: "https://api.modrinth.com/v2/project/nosuchplugin".to_owned(),
                status: 404,
                message: None,
            },
        };

        assert_eq!(
            modrinth_hint(&error).as_deref(),
            Some("run `mism plugin search <term>` to find the identifier")
        );
    }

    /// 「キャッシュのディレクトリのプラグインとデータパック」 方法: an invalid `source.toml` is an
    /// error whose hint is to remove its directory.
    #[test]
    fn invalid_source_toml_is_an_error_with_a_hint_to_remove_its_directory() {
        let directory = PathBuf::from("/cache/mism/plugins/Example").join("0".repeat(64));
        let error = CacheError::from(fetched_file_cache::Error::ParseSourceToml {
            path: directory.join("source.toml"),
            source: Box::new(toml::from_str::<toml::Table>("name =").unwrap_err()),
        });

        assert_eq!(
            error.hint(),
            Some(format!(
                "remove the directory {} and fetch the plugin again",
                directory.display()
            ))
        );
    }

    /// place-4: the hint is to move the jar out of `plugins/`.
    #[test]
    fn existing_jar_hints_at_moving_it_out_of_plugins() {
        let path = PathBuf::from("/srv/survival/plugins/Example.jar");

        let error = PlacedFilesError::from(placed_files::Error::FileExists { path: path.clone() });

        assert_eq!(
            error.hint(),
            Some(format!("move {} out of plugins/", path.display()))
        );
    }
}
