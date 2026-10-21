use super::{PlacedFilesError, SourceError, fetch, place};
use crate::command_line::PluginInstallArguments;
use crate::fetch_source::FetchSource;
use crate::interruption::Signal;
use crate::output;
use crate::plugin_compatibility::TargetServer;
use crate::prompt;
use crate::server_directory::ServerDirectory;
use crate::server_directory::plugins;
use crate::subcommands::target_server;

/// The answers that place the jar (inst-5).
const YES_ANSWERS: [&str; 2] = ["y", "yes"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error("stdin is not a terminal, so mism cannot ask whether to place the jar")]
    CannotConfirm,
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Fetch(#[from] fetch::Error),
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
            Error::CannotConfirm => Some("add --yes to place the jar without asking".to_owned()),
            Error::TargetServer(error) => error.hint(),
            Error::Source(error) => error.hint(),
            Error::Fetch(error) => error.hint(),
            Error::Place(error) => error.hint(),
            Error::PlacedFiles(error) => error.hint(),
            Error::Prompt(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Fetch(error) => error.interrupted_by(),
            Error::Place(error) => error.interrupted_by(),
            Error::Prompt(error) => error.interrupted_by(),
            Error::TargetServer(_)
            | Error::CannotConfirm
            | Error::Source(_)
            | Error::PlacedFiles(_)
            | Error::Output(_) => None,
        }
    }
}

/// `mism plugin install` (inst-1 to inst-7).
pub fn run(arguments: &PluginInstallArguments) -> Result<(), Error> {
    let server = target_server::required()?;
    check_confirmable(arguments.yes)?;
    let source = super::parse_source(&arguments.source)?;
    let lazy = lazy_target(&source, arguments.lazy, &server);
    let cached_jar = fetch::fetch_jar(&arguments.source, &source, arguments.yes, lazy)?;
    place::inspect_for_placement(&server, &cached_jar)?;
    plugins::placed_jars(&server)
        .check_placeable(&cached_jar.name, &cached_jar.file_name)
        .map_err(PlacedFilesError::from)?;
    if !confirm_placement(&cached_jar.file_name, arguments.yes)? {
        return Ok(());
    }
    Ok(place::place_jar(&server, &cached_jar)?)
}

/// Step 2: without `--yes`, the confirmation of step 7 needs a terminal, which is known before
/// any request (inst-7, d-install-1).
fn check_confirmable(yes: bool) -> Result<(), Error> {
    if yes || prompt::stdin_is_terminal() {
        Ok(())
    } else {
        Err(Error::CannotConfirm)
    }
}

/// Step 3: `--lazy` applies only to a `modrinth:` source without values (inst-2, inst-3).
fn lazy_target<'a>(
    source: &FetchSource,
    lazy: bool,
    server: &'a ServerDirectory,
) -> Option<TargetServer<'a>> {
    match source {
        FetchSource::Modrinth(modrinth) if lazy && modrinth.values.is_empty() => {
            Some(super::target_of(server))
        }
        FetchSource::Modrinth(_) | FetchSource::Url(_) | FetchSource::File(_) => None,
    }
}

/// Step 7: whether to go on to the placement; with `--yes` without asking (inst-6). An answer
/// other than `y` and `yes` writes that the jar is not placed (inst-5).
fn confirm_placement(file_name: &str, yes: bool) -> Result<bool, Error> {
    if yes {
        return Ok(true);
    }
    let answer = prompt::ask(&format!("place {file_name} in plugins/? (y/n): "))?;
    if YES_ANSWERS.contains(&answer.as_str()) {
        return Ok(true);
    }
    output::write_to_stdout(&format!("not placing {file_name}"))?;
    Ok(false)
}
