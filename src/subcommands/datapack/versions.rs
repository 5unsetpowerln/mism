use super::SourceError;
use crate::command_line::DatapackVersionsArguments;
use crate::datapack_compatibility;
use crate::interruption::Signal;
use crate::minecraft_version;
use crate::modrinth_source;
use crate::output::{self, Field};
use crate::subcommands::modrinth_versions::{self, Listed};
use crate::subcommands::target_server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Argument(#[from] SourceError),
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    Versions(#[from] modrinth_versions::Error),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Argument(error) => error.hint(),
            Error::TargetServer(error) => error.hint(),
            Error::Versions(modrinth_versions::Error::Modrinth(error)) => {
                super::modrinth_hint(error)
            }
            Error::Versions(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Versions(error) => error.interrupted_by(),
            Error::Argument(_) | Error::TargetServer(_) | Error::Output(_) => None,
        }
    }
}

/// `mism datapack versions` (dvers-1 to dvers-6). It writes the names of the values and the
/// versions only after every request succeeded (dvers-5).
pub fn run(arguments: &DatapackVersionsArguments) -> Result<(), Error> {
    let identifier =
        modrinth_source::parse_project(&arguments.datapack).map_err(SourceError::from)?;
    let server = target_server::current()?;
    let listed = modrinth_versions::listed(&identifier, super::is_datapack_version)?;
    let minecraft_version = server
        .as_ref()
        .map(|server| server.mism_toml.minecraft_version.as_str());
    write_names_of_the_values(minecraft_version)?;
    write_versions(&listed, minecraft_version)
}

/// Step 7: the names of the values of a version record, with `declared` at the end when there is a
/// target server, whose Minecraft version is `minecraft_version` (dvers-6, dp-12).
fn write_names_of_the_values(minecraft_version: Option<&str>) -> Result<(), Error> {
    let names = [
        "version",
        "release_type",
        "newest_minecraft_version",
        "required_dependencies",
    ]
    .into_iter()
    .chain(minecraft_version.map(|_| "declared"));
    let fields: Vec<Field> = names.map(|name| Field::Value(Some(name))).collect();
    Ok(output::write_to_stdout(&output::record(&fields))?)
}

/// Step 8: one record per datapack version (dvers-1, dvers-2, dvers-3, dp-12).
fn write_versions(listed: &[Listed], minecraft_version: Option<&str>) -> Result<(), Error> {
    let lines: Vec<String> = listed
        .iter()
        .map(|listed| version_line(listed, minecraft_version))
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

/// The version number, the release type, the newest Minecraft version, and the required
/// dependencies, then with a target server the agreement with its Minecraft version (dvers-2,
/// dvers-3).
fn version_line(listed: &Listed, minecraft_version: Option<&str>) -> String {
    let version = &listed.version;
    let agreement = minecraft_version.map(|minecraft_version| {
        datapack_compatibility::agreement(&version.game_versions, minecraft_version).to_string()
    });
    let mut fields = vec![
        Field::Value(Some(&version.version_number)),
        Field::Value(Some(&version.version_type)),
        Field::Value(minecraft_version::newest(&version.game_versions)),
        Field::List(
            listed
                .required_dependencies
                .iter()
                .map(String::as_str)
                .collect(),
        ),
    ];
    fields.extend(
        agreement
            .as_deref()
            .map(|agreement| Field::Value(Some(agreement))),
    );
    output::record(&fields)
}
