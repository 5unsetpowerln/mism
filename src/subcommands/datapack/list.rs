use super::{CacheError, PlacedFilesError};
use crate::command_line::DatapackListArguments;
use crate::fetched_file_cache::CachedFile;
use crate::interruption::Signal;
use crate::modrinth_source;
use crate::output::{self, Field};
use crate::server_directory::datapacks;
use crate::server_directory::placed_files::FileState;
use crate::subcommands::target_server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    World(#[from] datapacks::Error),
    #[error(transparent)]
    PlacedFiles(#[from] PlacedFilesError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::TargetServer(error) => error.hint(),
            Error::World(error) => error.hint(),
            Error::PlacedFiles(error) => error.hint(),
            Error::Cache(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Cache(error) => error.interrupted_by(),
            Error::TargetServer(_) | Error::World(_) | Error::PlacedFiles(_) | Error::Output(_) => {
                None
            }
        }
    }
}

/// `mism datapack list`, or with `--global` the datapack files in the cache directory.
pub fn run(arguments: &DatapackListArguments) -> Result<(), Error> {
    if arguments.global {
        write_cached_datapacks()
    } else {
        write_server_datapacks()
    }
}

/// dlist-1, dlist-2: the records and the datapack files of the world's `datapacks/`, held against
/// each other.
fn write_server_datapacks() -> Result<(), Error> {
    let server = target_server::required()?;
    let directory = datapacks::path(&server)?;
    let lines: Vec<String> = datapacks::placed_datapacks(&server, directory)
        .reconcile()
        .map_err(PlacedFilesError::from)?
        .iter()
        .map(server_datapack_line)
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

fn server_datapack_line(datapack: &FileState) -> String {
    let state = datapack.state.to_string();
    let source = datapack.source.as_deref();
    output::record(&[
        Field::Value(Some(&state)),
        Field::Value(Some(&datapack.name)),
        Field::Value(source.and_then(modrinth_source::version_number_of)),
        Field::Value(Some(&datapack.file_name)),
        Field::Value(source),
    ])
}

/// dlist-3, dlist-4: in the order of the datapack name, then the SHA-256 (d-list-1).
fn write_cached_datapacks() -> Result<(), Error> {
    let lines: Vec<String> = super::open_cache()?
        .list()
        .map_err(CacheError::from)?
        .iter()
        .map(cached_datapack_line)
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

fn cached_datapack_line(cached_file: &CachedFile) -> String {
    output::record(&[
        Field::Value(Some(&cached_file.name)),
        Field::Value(cached_file.version_number()),
        Field::Value(Some(&cached_file.sha256)),
        Field::Value(Some(&cached_file.source)),
    ])
}
