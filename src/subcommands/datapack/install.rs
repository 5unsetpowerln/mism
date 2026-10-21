use std::path::Path;

use super::{PlacedFilesError, SourceError, fetch, inspect, place};
use crate::command_line::DatapackInstallArguments;
use crate::fetch_source::FetchSource;
use crate::interruption::Signal;
use crate::output;
use crate::prompt;
use crate::server_directory::ServerDirectory;
use crate::server_directory::datapacks;
use crate::subcommands::target_server;

/// The answers that place the datapack file (dinst-5).
const YES_ANSWERS: [&str; 2] = ["y", "yes"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    World(#[from] datapacks::Error),
    #[error("stdin is not a terminal, so mism cannot ask whether to place the datapack")]
    CannotConfirm,
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Fetch(#[from] fetch::Error),
    #[error(transparent)]
    Inspect(#[from] inspect::Error),
    #[error(transparent)]
    Place(#[from] place::Error),
    #[error(transparent)]
    PlacedFiles(#[from] PlacedFilesError),
    #[error(transparent)]
    Prompt(#[from] prompt::Error),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::CannotConfirm => {
                Some("add --yes to place the datapack without asking".to_owned())
            }
            Error::TargetServer(error) => error.hint(),
            Error::World(error) => error.hint(),
            Error::Source(error) => error.hint(),
            Error::Fetch(error) => error.hint(),
            Error::Inspect(error) => error.hint(),
            Error::Place(error) => error.hint(),
            Error::PlacedFiles(error) => error.hint(),
            Error::Prompt(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Fetch(error) => error.interrupted_by(),
            Error::Inspect(error) => error.interrupted_by(),
            Error::Place(error) => error.interrupted_by(),
            Error::Prompt(error) => error.interrupted_by(),
            Error::TargetServer(_)
            | Error::World(_)
            | Error::CannotConfirm
            | Error::Source(_)
            | Error::PlacedFiles(_)
            | Error::Output(_) => None,
        }
    }
}

/// `mism datapack install` (dinst-1 to dinst-7).
pub fn run(arguments: &DatapackInstallArguments) -> Result<(), Error> {
    let server = target_server::required()?;
    let directory = datapacks::path(&server)?;
    check_confirmable(arguments.yes)?;
    let source = super::parse_source(&arguments.source)?;
    let lazy = lazy_target(&source, arguments.lazy, &server);
    let cached_file = fetch::fetch_file(&arguments.source, &source, arguments.yes, lazy)?;
    inspect::inspect_file(&cached_file, Some(&server))?;
    datapacks::placed_datapacks(&server, directory.clone())
        .check_placeable(&cached_file.name, &cached_file.file_name)
        .map_err(PlacedFilesError::from)?;
    if !confirm_placement(&directory, &cached_file.file_name, arguments.yes)? {
        return Ok(());
    }
    Ok(place::place_file(&server, &directory, &cached_file)?)
}

/// Step 2: without `--yes`, the confirmation of step 7 needs a terminal, which is known before
/// any request (dinst-7, d-install-1).
fn check_confirmable(yes: bool) -> Result<(), Error> {
    if yes || prompt::stdin_is_terminal() {
        Ok(())
    } else {
        Err(Error::CannotConfirm)
    }
}

/// Step 3: `--lazy` applies only to a `modrinth:` source without values (dinst-2, dinst-3).
fn lazy_target<'a>(
    source: &FetchSource,
    lazy: bool,
    server: &'a ServerDirectory,
) -> Option<&'a ServerDirectory> {
    match source {
        FetchSource::Modrinth(modrinth) if lazy && modrinth.values.is_empty() => Some(server),
        FetchSource::Modrinth(_) | FetchSource::Url(_) | FetchSource::File(_) => None,
    }
}

/// Step 7: whether to go on to the placement; with `--yes` without asking (dinst-6). An answer
/// other than `y` and `yes` writes that the file is not placed (dinst-5). The question shows the
/// world's `datapacks/` by its absolute path.
fn confirm_placement(directory: &Path, file_name: &str, yes: bool) -> Result<bool, Error> {
    if yes {
        return Ok(true);
    }
    let answer = prompt::ask(&format!(
        "place {file_name} in {}/? (y/n): ",
        directory.display()
    ))?;
    if YES_ANSWERS.contains(&answer.as_str()) {
        return Ok(true);
    }
    output::write_to_stdout(&format!("not placing {file_name}"))?;
    Ok(false)
}
