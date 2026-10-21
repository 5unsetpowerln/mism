use std::path::PathBuf;

use super::{CacheError, PlacedFilesError, SourceError, inspect};
use crate::command_line::PluginPlaceArguments;
use crate::fetched_file_cache::CachedFile;
use crate::interruption::{self, Interrupted, Signal};
use crate::output::{self, Field};
use crate::server_directory::ServerDirectory;
use crate::server_directory::placed_files::FileToPlace;
use crate::server_directory::plugins;
use crate::subcommands::target_server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("the cache directory has no jar fetched from {fetched_from}")]
    NotFetched { fetched_from: String },
    #[error(transparent)]
    Inspect(#[from] inspect::Error),
    #[error(
        "the inspection found {}, so {file_name} is not placed",
        found_judgements(*.incompatible, .conflict)
    )]
    PreventedByInspection {
        file_name: String,
        incompatible: bool,
        /// The paths of the conflicting jars in `plugins/`.
        conflict: Vec<PathBuf>,
    },
    #[error(transparent)]
    PlacedFiles(#[from] PlacedFilesError),
    #[error(transparent)]
    Interrupted(#[from] Interrupted),
    #[error(transparent)]
    Output(#[from] output::Error),
}

fn found_judgements(incompatible: bool, conflict: &[PathBuf]) -> &'static str {
    match (incompatible, conflict.is_empty()) {
        (true, true) => "`incompatible`",
        (true, false) => "`incompatible` and `conflict`",
        (false, _) => "`conflict`",
    }
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::NotFetched { fetched_from } => Some(format!(
                "run `mism plugin fetch {fetched_from}` and give the source it prints"
            )),
            Error::PreventedByInspection { conflict, .. } if !conflict.is_empty() => Some(format!(
                "move {} out of plugins/",
                conflict
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Error::TargetServer(error) => error.hint(),
            Error::Source(error) => error.hint(),
            Error::Cache(error) => error.hint(),
            Error::Inspect(error) => error.hint(),
            Error::PlacedFiles(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::PreventedByInspection { .. } | Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Cache(error) => error.interrupted_by(),
            Error::Inspect(error) => error.interrupted_by(),
            Error::Interrupted(Interrupted(signal)) => Some(*signal),
            Error::TargetServer(_)
            | Error::Source(_)
            | Error::NotFetched { .. }
            | Error::PreventedByInspection { .. }
            | Error::PlacedFiles(_)
            | Error::Output(_) => None,
        }
    }
}

/// `mism plugin place`. Nothing in it communicates (place-1).
pub fn run(arguments: &PluginPlaceArguments) -> Result<(), Error> {
    let server = target_server::required()?;
    let cached_jar = find_fetched(&arguments.source)?;
    inspect_for_placement(&server, &cached_jar)?;
    plugins::placed_jars(&server)
        .check_placeable(&cached_jar.name, &cached_jar.file_name)
        .map_err(PlacedFilesError::from)?;
    place_jar(&server, &cached_jar)
}

/// Step 2 (place-1).
fn find_fetched(text: &str) -> Result<CachedFile, Error> {
    let source = super::parse_source(text)?;
    super::find_in_cache(text, &source)?.ok_or_else(|| Error::NotFetched {
        fetched_from: text.to_owned(),
    })
}

/// Step 3: the inspection procedure, after which `incompatible` or `conflict` stops the
/// placement (place-6, inst-4).
pub fn inspect_for_placement(
    server: &ServerDirectory,
    cached_jar: &CachedFile,
) -> Result<(), Error> {
    let inspection = inspect::write_inspection(cached_jar)?;
    let judgement = inspect::write_judgement(&inspection, server)?;
    if !judgement.prevents_placement() {
        return Ok(());
    }
    Err(Error::PreventedByInspection {
        file_name: cached_jar.file_name.clone(),
        incompatible: !judgement.incompatible.is_empty(),
        conflict: judgement
            .conflict
            .iter()
            .map(|file_name| plugins::path(server).join(file_name))
            .collect(),
    })
}

/// Steps 5 to 7, the placement procedure (配置の手順) that `install` uses too (place-2, place-5,
/// plg-7, plg-8).
pub fn place_jar(server: &ServerDirectory, cached_jar: &CachedFile) -> Result<(), Error> {
    interruption::fail_if_received()?;
    plugins::placed_jars(server)
        .place(&FileToPlace {
            cached_path: &cached_jar.path,
            sha256: &cached_jar.sha256,
            name: &cached_jar.name,
            source: &cached_jar.source,
            file_name: &cached_jar.file_name,
        })
        .map_err(PlacedFilesError::from)?;
    write_placed(cached_jar)?;
    Ok(super::write_takes_effect_notice()?)
}

/// place-5: the plugin name, the version number, and the jar file name.
fn write_placed(cached_jar: &CachedFile) -> Result<(), Error> {
    Ok(output::write_to_stdout(&output::record(&[
        Field::Value(Some(&cached_jar.name)),
        Field::Value(cached_jar.version_number()),
        Field::Value(Some(&cached_jar.file_name)),
    ]))?)
}
