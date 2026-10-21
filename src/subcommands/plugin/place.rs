use std::path::PathBuf;

use super::{CommonStepError, inspect};
use crate::command_line::PlaceArguments;
use crate::interruption::Signal;
use crate::output::{self, Field};
use crate::plugin_cache::CachedJar;
use crate::plugin_source;
use crate::server_directory::ServerDirectory;
use crate::server_directory::plugins::{self, JarToPlace};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Source(#[from] plugin_source::Error),
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
    Plugins(#[from] plugins::Error),
    #[error(transparent)]
    CommonStep(#[from] CommonStepError),
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
            Error::Source(error) => error.hint(),
            Error::Inspect(error) => error.hint(),
            Error::Plugins(error) => error.hint(),
            Error::CommonStep(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::PreventedByInspection { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Inspect(error) => error.interrupted_by(),
            Error::CommonStep(error) => error.interrupted_by(),
            Error::Source(_)
            | Error::NotFetched { .. }
            | Error::PreventedByInspection { .. }
            | Error::Plugins(_)
            | Error::Output(_) => None,
        }
    }
}

/// `mism plugin place`. Nothing in it communicates (place-1).
pub fn run(arguments: &PlaceArguments) -> Result<(), Error> {
    let server = super::required_target_server()?;
    let cached_jar = find_fetched(&arguments.source)?;
    inspect_for_placement(&server, &cached_jar)?;
    plugins::check_placeable(&server, &cached_jar.name, &cached_jar.file_name)?;
    place_jar(&server, &cached_jar)
}

/// Step 2 (place-1).
fn find_fetched(text: &str) -> Result<CachedJar, Error> {
    let source = plugin_source::parse(text)?;
    inspect::find_in_cache(text, &source)?.ok_or_else(|| Error::NotFetched {
        fetched_from: text.to_owned(),
    })
}

/// Step 3: the inspection procedure, after which `incompatible` or `conflict` stops the
/// placement (place-6, inst-4).
pub fn inspect_for_placement(
    server: &ServerDirectory,
    cached_jar: &CachedJar,
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
            .map(|file_name| server.plugins_path().join(file_name))
            .collect(),
    })
}

/// Steps 5 to 7, the placement procedure (配置の手順) that `install` uses too (place-2, place-5,
/// plg-7, plg-8).
pub fn place_jar(server: &ServerDirectory, cached_jar: &CachedJar) -> Result<(), Error> {
    super::fail_if_interrupted()?;
    plugins::place(
        server,
        &JarToPlace {
            cached_path: &cached_jar.path,
            sha256: &cached_jar.sha256,
            plugin_name: &cached_jar.name,
            source: &cached_jar.source,
            file_name: &cached_jar.file_name,
        },
    )?;
    write_placed(cached_jar)?;
    Ok(super::write_takes_effect_notice()?)
}

/// place-5: the plugin name, the version number, and the jar file name.
fn write_placed(cached_jar: &CachedJar) -> Result<(), Error> {
    Ok(output::write_to_stdout(&output::record(&[
        Field::Value(Some(&cached_jar.name)),
        Field::Value(plugin_source::version_number_of(&cached_jar.source)),
        Field::Value(Some(&cached_jar.file_name)),
    ]))?)
}
