use super::PlacedFilesError;
use crate::command_line::PluginRemoveArguments;
use crate::interruption::{self, Interrupted, Signal};
use crate::output;
use crate::server_directory::mism_toml::Record;
use crate::server_directory::placed_files::Removed;
use crate::server_directory::plugins;
use crate::subcommands::target_server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    Interrupted(#[from] Interrupted),
    #[error(transparent)]
    PlacedFiles(#[from] PlacedFilesError),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::TargetServer(error) => error.hint(),
            Error::PlacedFiles(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interrupted(Interrupted(signal)) => Some(*signal),
            Error::TargetServer(_) | Error::PlacedFiles(_) | Error::Output(_) => None,
        }
    }
}

/// `mism plugin remove` (rm-1 to rm-3).
pub fn run(arguments: &PluginRemoveArguments) -> Result<(), Error> {
    let server = target_server::required()?;
    interruption::fail_if_received()?;
    match plugins::placed_jars(&server)
        .remove(&arguments.name)
        .map_err(PlacedFilesError::from)?
    {
        Removed::FileAndRecord(_) => super::write_takes_effect_notice()?,
        Removed::RecordOnly(record) => output::write_to_stdout(&missing_jar_notice(&record))?,
    }
    Ok(())
}

/// rm-3: the record was removed, its jar was not in `plugins/`.
fn missing_jar_notice(record: &Record) -> String {
    format!(
        "plugins/{} does not exist, so only the record of {} was removed",
        record.file_name, record.name
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// rm-3: `plugins/` is shown by its name.
    #[test]
    fn missing_jar_is_shown_in_plugins_by_name() {
        let record = Record {
            name: "A".to_owned(),
            source: "https://example.com/A.jar".to_owned(),
            file_name: "A.jar".to_owned(),
            sha256: "0".repeat(64),
        };

        assert_eq!(
            missing_jar_notice(&record),
            "plugins/A.jar does not exist, so only the record of A was removed"
        );
    }
}
