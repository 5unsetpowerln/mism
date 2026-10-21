use super::CommonStepError;
use crate::command_line::RemoveArguments;
use crate::interruption::Signal;
use crate::output;
use crate::server_directory::plugins::{self, Removed};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Plugins(#[from] plugins::Error),
    #[error(transparent)]
    CommonStep(#[from] CommonStepError),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Plugins(error) => error.hint(),
            Error::CommonStep(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::CommonStep(error) => error.interrupted_by(),
            Error::Plugins(_) | Error::Output(_) => None,
        }
    }
}

/// `mism plugin remove` (rm-1 to rm-3).
pub fn run(arguments: &RemoveArguments) -> Result<(), Error> {
    let server = super::required_target_server()?;
    super::fail_if_interrupted()?;
    match plugins::remove(&server, &arguments.name)? {
        Removed::JarAndRecord(_) => super::write_takes_effect_notice()?,
        Removed::RecordOnly(record) => output::write_to_stdout(&format!(
            "plugins/{} does not exist, so only the record of {} was removed",
            record.file_name, record.name
        ))?,
    }
    Ok(())
}
