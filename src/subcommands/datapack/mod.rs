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

use crate::command_line::{DatapackArguments, DatapackSubcommand};
use crate::fetch_source::{self, FetchSource};
use crate::fetched_file_cache::{self, CachedFile, FetchedFileCache};
use crate::file_name;
use crate::interruption::Signal;
use crate::modrinth_api;
use crate::modrinth_api::response::Version;
use crate::modrinth_source;
use crate::output;
use crate::server_directory::placed_files;

/// The end of a datapack file's name (dp-4, dp-8).
const ZIP_SUFFIX: &str = ".zip";
/// The directory of the cache directory that holds the fetched datapack files.
const CACHE_DIRECTORY_NAME: &str = "datapacks";
/// What Modrinth puts in the `loaders` of a datapack's version (dsrch-2, dvers-1, dp-5).
const DATAPACK_LOADER: &str = "datapack";
const THREE_FORMS: &str = "give `modrinth:<identifier>` (optionally followed by `:<value>`s), a \
                           datapack URL starting with `https://` or `http://`, or the path of a \
                           local datapack file";
const SEARCH_HINT: &str = "run `mism datapack search <term>` to find the identifier";

/// What a `mism datapack` subcommand reports. The subcommands' errors are boxed: they are large,
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

pub fn run(arguments: &DatapackArguments) -> Result<(), Error> {
    match &arguments.subcommand {
        DatapackSubcommand::Search(arguments) => Ok(search::run(arguments).map_err(Box::new)?),
        DatapackSubcommand::Versions(arguments) => Ok(versions::run(arguments).map_err(Box::new)?),
        DatapackSubcommand::Fetch(arguments) => Ok(fetch::run(arguments).map_err(Box::new)?),
        DatapackSubcommand::Inspect(arguments) => Ok(inspect::run(arguments).map_err(Box::new)?),
        DatapackSubcommand::Place(arguments) => Ok(place::run(arguments).map_err(Box::new)?),
        DatapackSubcommand::Install(arguments) => Ok(install::run(arguments).map_err(Box::new)?),
        DatapackSubcommand::List(arguments) => Ok(list::run(arguments).map_err(Box::new)?),
        DatapackSubcommand::Remove(arguments) => Ok(remove::run(arguments).map_err(Box::new)?),
    }
}

/// Whether a Modrinth version is a datapack's, which the search describes, `versions` lists, and
/// whose files are candidates (dsrch-2, dvers-1, dp-5).
fn is_datapack_version(version: &Version) -> bool {
    version
        .loaders
        .iter()
        .any(|loader| loader == DATAPACK_LOADER)
}

/// A datapack's <取得元の指定> (dp-4).
fn parse_source(text: &str) -> Result<FetchSource, SourceError> {
    Ok(fetch_source::parse(text, ZIP_SUFFIX)?)
}

/// The datapack files fetched into the cache directory.
fn open_cache() -> Result<FetchedFileCache, CacheError> {
    Ok(FetchedFileCache::open(CACHE_DIRECTORY_NAME)?)
}

/// The datapack file fetched with the source `text`, or for a local datapack file the cached file
/// with its content (dplace-1, dinsp-1).
fn find_in_cache(text: &str, source: &FetchSource) -> Result<Option<CachedFile>, CacheError> {
    Ok(open_cache()?.find(text, source.local_path())?)
}

/// The hint for a failed request to Modrinth: for an identifier of no project, the search
/// (dp-4).
fn modrinth_hint(error: &modrinth_api::Error) -> Option<String> {
    match error {
        modrinth_api::Error::ProjectNotFound { .. } => Some(SEARCH_HINT.to_owned()),
        error => error.hint(),
    }
}

/// Written after the result of a subcommand that changed the world's `datapacks/` (dp-10).
fn write_takes_effect_notice() -> Result<(), output::Error> {
    output::write_to_stdout("the change takes effect when the server starts next")
}

/// The hint for a cached file that cannot be used: to remove its SHA-256 directory.
fn refetch_hint(path: &Path) -> String {
    format!(
        "remove the directory {} and fetch the datapack again",
        path.parent()
            .expect("a file of the cache directory is in a SHA-256 directory")
            .display()
    )
}

/// A datapack name or a datapack file name that cannot be used (dp-9).
#[derive(Debug, thiserror::Error)]
pub enum NameError {
    #[error("the datapack name {:?} cannot be used: {}", .0.name(), .0.reason())]
    DatapackName(file_name::Error),
    #[error("the datapack file name {:?} cannot be used: {}", .0.name(), .0.reason())]
    DatapackFileName(file_name::Error),
}

impl From<file_name::Error> for NameError {
    fn from(error: file_name::Error) -> NameError {
        match error {
            file_name::Error::Name { .. } => NameError::DatapackName(error),
            file_name::Error::FileName { .. } => NameError::DatapackFileName(error),
        }
    }
}

/// A datapack's source that is none of its three forms (dp-4).
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error(
        "{} is not a source: failed to examine it as a local datapack file",
        path.display()
    )]
    ExamineLocalFile {
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
                SourceError::ExamineLocalFile { path, source }
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
            SourceError::ExamineLocalFile { .. }
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

/// A failure of the datapack files in the cache directory.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error(
        "several directories of the cache directory hold the datapack with the SHA-256 {sha256}: \
         {}",
        fetched_file_cache::list_paths(.paths)
    )]
    DuplicateSha256 { sha256: String, paths: Vec<PathBuf> },
    #[error(
        "the cache directory has several datapacks fetched from {fetched_from}: {}",
        fetched_file_cache::list_paths(.paths)
    )]
    SeveralDatapacks {
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
            } => CacheError::SeveralDatapacks {
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
            CacheError::SeveralDatapacks { .. } => Some(
                "give the path of one of these datapacks in the cache directory as the source"
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
            | CacheError::SeveralDatapacks { .. }
            | CacheError::Name(_)
            | CacheError::RecordedFileName { .. } => None,
        }
    }
}

/// A failure of the datapack files in the world's `datapacks/` and their records.
#[derive(Debug, thiserror::Error)]
pub enum PlacedFilesError {
    #[error("mism.toml already has a record of the datapack {name}")]
    RecordExists { name: String },
    #[error("mism.toml has no record of the datapack {name}")]
    RecordNotFound { name: String },
    #[error("mism.toml has several records of the datapack {name}")]
    SeveralRecords { name: String },
    #[error("the file name in the record of {name} cannot be used")]
    RecordedFileName {
        name: String,
        #[source]
        source: NameError,
    },
    #[error(
        "failed to remove {}, to which the removed datapack's file was renamed",
        path.display()
    )]
    RemoveRenamedFile {
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
                PlacedFilesError::RemoveRenamedFile { path, source }
            }
            error => PlacedFilesError::PlacedFiles(error),
        }
    }
}

impl PlacedFilesError {
    pub fn hint(&self) -> Option<String> {
        match self {
            PlacedFilesError::RecordExists { name } => Some(format!(
                "run `mism datapack remove {name}` to remove the record first"
            )),
            PlacedFilesError::RecordNotFound { .. } => {
                Some("run `mism datapack list` to see the names in the records".to_owned())
            }
            PlacedFilesError::PlacedFiles(error) => match error.failure() {
                placed_files::Error::FileExists { path } => {
                    Some(format!("move {} out of datapacks/", path.display()))
                }
                placed_files::Error::CachedFileSha256Mismatch { path, .. } => {
                    Some(refetch_hint(path))
                }
                _ => error.hint(),
            },
            PlacedFilesError::SeveralRecords { .. }
            | PlacedFilesError::RecordedFileName { .. }
            | PlacedFilesError::RemoveRenamedFile { .. } => None,
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

    fn version_for(loaders: &[&str]) -> Version {
        Version {
            id: "v".to_owned(),
            version_number: "1.0".to_owned(),
            version_type: "release".to_owned(),
            loaders: loaders.iter().map(|loader| (*loader).to_owned()).collect(),
            game_versions: vec!["26.3".to_owned()],
            date_published: "2026-10-21T00:00:00Z".parse().unwrap(),
            dependencies: Vec::new(),
            files: Vec::new(),
        }
    }

    /// dsrch-2, dvers-1: datapacks are described and listed by datapack versions only.
    #[test]
    fn datapack_versions_are_the_ones_with_the_datapack_loader() {
        assert_eq!(
            (
                is_datapack_version(&version_for(&["datapack", "minecraft"])),
                is_datapack_version(&version_for(&["fabric", "forge"]))
            ),
            (true, false)
        );
    }

    /// dp-4: the hint for a datapack names the datapack forms.
    #[test]
    fn missing_local_datapack_file_hints_at_the_datapack_forms() {
        let directory = temporary_directory();
        let missing = directory.path().join("missing.zip");

        let error = parse_source(missing.to_str().unwrap()).unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some(
                "give `modrinth:<identifier>` (optionally followed by `:<value>`s), a datapack \
                 URL starting with `https://` or `http://`, or the path of a local datapack file"
            )
        );
    }

    /// d-source-3: the identifier is checked, with a hint at the datapack search.
    #[test]
    fn invalid_identifier_hints_at_the_datapack_search() {
        assert_eq!(
            parse_source("modrinth:../search")
                .unwrap_err()
                .hint()
                .as_deref(),
            Some("run `mism datapack search <term>` to find the identifier")
        );
    }

    /// dvers-4: the argument of `mism datapack versions` is checked, with a hint at the datapack
    /// search.
    #[test]
    fn versions_argument_with_an_invalid_identifier_hints_at_the_search() {
        let error = SourceError::from(modrinth_source::parse_project("modrinth:te").unwrap_err());

        assert_eq!(
            error.hint().as_deref(),
            Some("run `mism datapack search <term>` to find the identifier")
        );
    }

    /// 「コマンドラインとエラーの出力」: a missing datapack project hints at `mism datapack search`.
    #[test]
    fn missing_datapack_project_hints_at_the_datapack_search() {
        let error = modrinth_api::Error::ProjectNotFound {
            identifier: "nosuchpack".to_owned(),
            source: http_client::Error::Status {
                url: "https://api.modrinth.com/v2/project/nosuchpack".to_owned(),
                status: 404,
                message: None,
            },
        };

        assert_eq!(
            modrinth_hint(&error).as_deref(),
            Some("run `mism datapack search <term>` to find the identifier")
        );
    }

    /// dp-9: the error names the datapack name it was checked for.
    #[test]
    fn error_names_the_noun_it_was_checked_for() {
        let error = NameError::from(file_name::check_name("..").unwrap_err());

        assert_eq!(
            error.to_string(),
            "the datapack name \"..\" cannot be used: it is empty, `.`, `..`, or contains NUL"
        );
    }

    /// drm-2: the hint names `mism datapack list`.
    #[test]
    fn removal_of_an_unrecorded_datapack_hints_at_the_datapack_list() {
        let error = PlacedFilesError::from(placed_files::Error::RecordNotFound {
            name: "Pack".to_owned(),
        });

        assert_eq!(
            error.hint().as_deref(),
            Some("run `mism datapack list` to see the names in the records")
        );
    }
}
