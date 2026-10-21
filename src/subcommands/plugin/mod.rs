mod fetch;
mod inspect;
mod install;
mod list;
mod place;
mod remove;
mod search;
mod versions;

use crate::command_line::{PluginArguments, PluginSubcommand};
use crate::compatibility::TargetServer;
use crate::interruption::{self, Signal};
use crate::modrinth_api::selection::{AskError, Asker};
use crate::output;
use crate::prompt;
use crate::server_directory::{self, ServerDirectory};

const TAKES_EFFECT_NOTICE: &str = "the change takes effect when the server starts next";

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

/// A failure of the steps the subcommands share: determining the target server, and checking
/// for a signal (「mism plugin の対象のサーバーと共通の手順」).
#[derive(Debug, thiserror::Error)]
pub enum CommonStepError {
    #[error("the current directory has no mism.toml, so it is not a server directory")]
    NoTargetServer,
    #[error(transparent)]
    ServerDirectory(#[from] server_directory::Error),
    #[error("interrupted by {0}")]
    Interrupted(Signal),
}

impl CommonStepError {
    pub fn hint(&self) -> Option<String> {
        match self {
            CommonStepError::NoTargetServer => {
                Some("run the command in a server directory that `mism new` created".to_owned())
            }
            CommonStepError::ServerDirectory(error) => error.hint(),
            CommonStepError::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            CommonStepError::Interrupted(signal) => Some(*signal),
            CommonStepError::NoTargetServer | CommonStepError::ServerDirectory(_) => None,
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

/// Step 1 of 「mism plugin の対象のサーバーと共通の手順」: the current directory as the target
/// server, `None` when it has no `mism.toml`. `versions` and `inspect` go on without one (plg-2).
pub fn target_server() -> Result<Option<ServerDirectory>, CommonStepError> {
    Ok(ServerDirectory::current()?)
}

/// Step 2: `place`, `install`, `remove`, and `list` without `--global` need the target server
/// (plg-1).
pub fn required_target_server() -> Result<ServerDirectory, CommonStepError> {
    target_server()?.ok_or(CommonStepError::NoTargetServer)
}

/// The server software and the Minecraft version of the target server, to hold plugins against
/// (vers-3, insp-3, inst-2).
pub fn target_of(server: &ServerDirectory) -> TargetServer<'_> {
    TargetServer {
        software: &server.mism_toml.software,
        minecraft_version: &server.mism_toml.minecraft_version,
    }
}

/// Checked right before a subcommand starts to change the cache directory, `plugins/`, or
/// `mism.toml`, and never after it started (d-pcmd-1).
pub fn fail_if_interrupted() -> Result<(), CommonStepError> {
    fail_if_received(interruption::received())
}

fn fail_if_received(received: Option<Signal>) -> Result<(), CommonStepError> {
    match received {
        Some(signal) => Err(CommonStepError::Interrupted(signal)),
        None => Ok(()),
    }
}

/// Written after the result of a subcommand that changed `plugins/` (plg-7).
pub fn write_takes_effect_notice() -> Result<(), output::Error> {
    output::write_to_stdout(TAKES_EFFECT_NOTICE)
}

/// Asks through stdin only when it is a terminal and the user did not give `--yes` (plg-11).
pub struct TerminalAsker {
    yes: bool,
}

impl TerminalAsker {
    pub fn new(yes: bool) -> TerminalAsker {
        TerminalAsker { yes }
    }
}

impl Asker for TerminalAsker {
    fn ask(&mut self, question: &str) -> Result<Option<String>, AskError> {
        if self.yes || !prompt::stdin_is_terminal() {
            return Ok(None);
        }
        prompt::ask(question)
            .map(Some)
            .map_err(|error| match error.interrupted_by() {
                Some(signal) => AskError::Interrupted(signal),
                None => AskError::Failed(Box::new(error)),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// d-pcmd-1: a signal that arrived stops the subcommand before it changes anything.
    #[test]
    fn received_signal_stops_before_changes() {
        assert!(matches!(
            fail_if_received(Some(Signal::Terminate)),
            Err(CommonStepError::Interrupted(Signal::Terminate))
        ));
    }

    /// d-pcmd-1: without a signal the subcommand goes on.
    #[test]
    fn no_signal_lets_the_subcommand_go_on() {
        assert!(fail_if_received(None).is_ok());
    }

    /// plg-11: with `--yes` the user is not asked.
    #[test]
    fn yes_means_the_user_cannot_be_asked() {
        assert_eq!(TerminalAsker::new(true).ask("question: ").unwrap(), None);
    }

    /// plg-1: the error without a target server hints at `mism new`.
    #[test]
    fn missing_target_server_hints_at_mism_new() {
        assert_eq!(
            CommonStepError::NoTargetServer.hint().as_deref(),
            Some("run the command in a server directory that `mism new` created")
        );
    }
}
