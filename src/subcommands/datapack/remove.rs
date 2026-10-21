use std::path::Path;

use super::PlacedFilesError;
use crate::command_line::DatapackRemoveArguments;
use crate::interruption::{self, Interrupted, Signal};
use crate::output;
use crate::server_directory::datapacks;
use crate::server_directory::mism_toml::Record;
use crate::server_directory::placed_files::Removed;
use crate::subcommands::target_server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    World(#[from] datapacks::Error),
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
            Error::World(error) => error.hint(),
            Error::PlacedFiles(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interrupted(Interrupted(signal)) => Some(*signal),
            Error::TargetServer(_) | Error::World(_) | Error::PlacedFiles(_) | Error::Output(_) => {
                None
            }
        }
    }
}

/// `mism datapack remove` (drm-1 to drm-3).
pub fn run(arguments: &DatapackRemoveArguments) -> Result<(), Error> {
    let server = target_server::required()?;
    let directory = datapacks::path(&server)?;
    interruption::fail_if_received()?;
    match datapacks::placed_datapacks(&server, directory.clone())
        .remove(&arguments.name)
        .map_err(PlacedFilesError::from)?
    {
        Removed::FileAndRecord(_) => super::write_takes_effect_notice()?,
        Removed::RecordOnly(record) => {
            output::write_to_stdout(&missing_file_notice(&directory, &record))?;
        }
    }
    Ok(())
}

/// drm-3: the record was removed, its file was not in `directory`, the world's `datapacks/`,
/// which the notice shows by its absolute path.
fn missing_file_notice(directory: &Path, record: &Record) -> String {
    format!(
        "{}/{} does not exist, so only the record of {} was removed",
        directory.display(),
        record.file_name,
        record.name
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// drm-3: the world's `datapacks` is shown by its absolute path.
    #[test]
    fn missing_file_is_shown_in_the_world_datapacks_by_absolute_path() {
        let record = Record {
            name: "A".to_owned(),
            source: "https://example.com/A.zip".to_owned(),
            file_name: "A.zip".to_owned(),
            sha256: "0".repeat(64),
        };

        assert_eq!(
            missing_file_notice(Path::new("/srv/survival/world/datapacks"), &record),
            "/srv/survival/world/datapacks/A.zip does not exist, so only the record of A was \
             removed"
        );
    }
}
