use super::{CacheError, PlacedFilesError};
use crate::command_line::PluginListArguments;
use crate::fetched_file_cache::CachedFile;
use crate::interruption::Signal;
use crate::modrinth_source;
use crate::output::{self, Field};
use crate::server_directory::placed_files::FileState;
use crate::server_directory::plugins;
use crate::subcommands::target_server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
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
            Error::PlacedFiles(error) => error.hint(),
            Error::Cache(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Cache(error) => error.interrupted_by(),
            Error::TargetServer(_) | Error::PlacedFiles(_) | Error::Output(_) => None,
        }
    }
}

/// `mism plugin list`, or with `--global` the plugins in the cache directory.
pub fn run(arguments: &PluginListArguments) -> Result<(), Error> {
    if arguments.global {
        write_cached_plugins()
    } else {
        write_server_plugins()
    }
}

/// list-1, list-2: the records and the jars of `plugins/`, held against each other.
fn write_server_plugins() -> Result<(), Error> {
    let server = target_server::required()?;
    let lines: Vec<String> = plugins::placed_jars(&server)
        .reconcile()
        .map_err(PlacedFilesError::from)?
        .iter()
        .map(server_plugin_line)
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

fn server_plugin_line(plugin: &FileState) -> String {
    let state = plugin.state.to_string();
    let source = plugin.source.as_deref();
    output::record(&[
        Field::Value(Some(&state)),
        Field::Value(Some(&plugin.name)),
        Field::Value(source.and_then(modrinth_source::version_number_of)),
        Field::Value(Some(&plugin.file_name)),
        Field::Value(source),
    ])
}

/// list-3, list-4: in the order of the plugin name, then the SHA-256 (d-list-1).
fn write_cached_plugins() -> Result<(), Error> {
    let lines: Vec<String> = super::open_cache()?
        .list()
        .map_err(CacheError::from)?
        .iter()
        .map(cached_plugin_line)
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

fn cached_plugin_line(cached_jar: &CachedFile) -> String {
    output::record(&[
        Field::Value(Some(&cached_jar.name)),
        Field::Value(cached_jar.version_number()),
        Field::Value(Some(&cached_jar.sha256)),
        Field::Value(Some(&cached_jar.source)),
    ])
}
