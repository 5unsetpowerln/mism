use std::path::Path;

use super::{CacheError, PlacedFilesError, SourceError, inspect};
use crate::command_line::DatapackPlaceArguments;
use crate::fetched_file_cache::CachedFile;
use crate::interruption::{self, Interrupted, Signal};
use crate::output::{self, Field};
use crate::server_directory::ServerDirectory;
use crate::server_directory::datapacks;
use crate::server_directory::placed_files::FileToPlace;
use crate::subcommands::target_server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    World(#[from] datapacks::Error),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("the cache directory has no datapack fetched from {fetched_from}")]
    NotFetched { fetched_from: String },
    #[error(transparent)]
    Inspect(#[from] inspect::Error),
    #[error(transparent)]
    PlacedFiles(#[from] PlacedFilesError),
    #[error(transparent)]
    Interrupted(#[from] Interrupted),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::NotFetched { fetched_from } => Some(format!(
                "run `mism datapack fetch {fetched_from}` and give the source it prints"
            )),
            Error::TargetServer(error) => error.hint(),
            Error::World(error) => error.hint(),
            Error::Source(error) => error.hint(),
            Error::Cache(error) => error.hint(),
            Error::Inspect(error) => error.hint(),
            Error::PlacedFiles(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Cache(error) => error.interrupted_by(),
            Error::Inspect(error) => error.interrupted_by(),
            Error::Interrupted(Interrupted(signal)) => Some(*signal),
            Error::TargetServer(_)
            | Error::World(_)
            | Error::Source(_)
            | Error::NotFetched { .. }
            | Error::PlacedFiles(_)
            | Error::Output(_) => None,
        }
    }
}

/// `mism datapack place`. Nothing in it communicates (dplace-1).
pub fn run(arguments: &DatapackPlaceArguments) -> Result<(), Error> {
    let server = target_server::required()?;
    let directory = datapacks::path(&server)?;
    let cached_file = find_fetched(&arguments.source)?;
    inspect::inspect_file(&cached_file, Some(&server))?;
    datapacks::placed_datapacks(&server, directory.clone())
        .check_placeable(&cached_file.name, &cached_file.file_name)
        .map_err(PlacedFilesError::from)?;
    place_file(&server, &directory, &cached_file)
}

/// Step 2 (dplace-1).
fn find_fetched(text: &str) -> Result<CachedFile, Error> {
    let source = super::parse_source(text)?;
    super::find_in_cache(text, &source)?.ok_or_else(|| Error::NotFetched {
        fetched_from: text.to_owned(),
    })
}

/// Steps 5 to 7, the placement procedure (配置の手順) that `install` uses too: in `directory`,
/// the world's `datapacks/` (dplace-2, dplace-5, dp-10, dp-11).
pub fn place_file(
    server: &ServerDirectory,
    directory: &Path,
    cached_file: &CachedFile,
) -> Result<(), Error> {
    interruption::fail_if_received()?;
    datapacks::placed_datapacks(server, directory.to_path_buf())
        .place(&FileToPlace {
            cached_path: &cached_file.path,
            sha256: &cached_file.sha256,
            name: &cached_file.name,
            source: &cached_file.source,
            file_name: &cached_file.file_name,
        })
        .map_err(PlacedFilesError::from)?;
    write_placed(cached_file)?;
    Ok(super::write_takes_effect_notice()?)
}

/// dplace-5: the datapack name, the version number, and the datapack file name.
fn write_placed(cached_file: &CachedFile) -> Result<(), Error> {
    Ok(output::write_to_stdout(&output::record(&[
        Field::Value(Some(&cached_file.name)),
        Field::Value(cached_file.version_number()),
        Field::Value(Some(&cached_file.file_name)),
    ]))?)
}
