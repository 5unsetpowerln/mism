use super::SourceError;
use crate::command_line::PluginVersionsArguments;
use crate::interruption::Signal;
use crate::minecraft_version;
use crate::modrinth_api::response::Version;
use crate::modrinth_source;
use crate::output::{self, Field};
use crate::plugin_compatibility::{self, TargetServer};
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

/// `mism plugin versions` (vers-1 to vers-6). It writes the names of the values and the versions
/// only after every request succeeded (vers-5).
pub fn run(arguments: &PluginVersionsArguments) -> Result<(), Error> {
    let identifier =
        modrinth_source::parse_project(&arguments.plugin).map_err(SourceError::from)?;
    let server = target_server::current()?;
    let listed = modrinth_versions::listed(&identifier, is_listed)?;
    let target = server.as_ref().map(super::target_of);
    write_names_of_the_values(target.as_ref())?;
    write_versions(&listed, target.as_ref())
}

/// Step 4: `mism plugin versions` lists every version of a plugin (vers-1).
fn is_listed(_version: &Version) -> bool {
    true
}

/// Step 7: the names of the values of a version record, with `declared` at the end when there is a
/// target server (vers-6, plg-9).
fn write_names_of_the_values(target: Option<&TargetServer>) -> Result<(), Error> {
    let names = [
        "version",
        "release_type",
        "software",
        "newest_minecraft_version",
        "required_dependencies",
    ]
    .into_iter()
    .chain(target.map(|_| "declared"));
    let fields: Vec<Field> = names.map(|name| Field::Value(Some(name))).collect();
    Ok(output::write_to_stdout(&output::record(&fields))?)
}

/// Step 8: one record per version (vers-1, vers-2, vers-3, plg-9).
fn write_versions(listed: &[Listed], target: Option<&TargetServer>) -> Result<(), Error> {
    let lines: Vec<String> = listed
        .iter()
        .map(|listed| version_line(listed, target))
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

/// The version number, the release type, the server software, the newest Minecraft version, and
/// the required dependencies, then with a target server the agreement with it (vers-2, vers-3).
fn version_line(listed: &Listed, target: Option<&TargetServer>) -> String {
    let version = &listed.version;
    let agreement = target.map(|target| {
        plugin_compatibility::agreement(&version.loaders, &version.game_versions, target)
            .to_string()
    });
    let mut fields = vec![
        Field::Value(Some(&version.version_number)),
        Field::Value(Some(&version.version_type)),
        Field::List(version.loaders.iter().map(String::as_str).collect()),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// vers-1: `versions` lists every version of a plugin.
    #[test]
    fn plugin_versions_are_all_listed() {
        let version = Version {
            id: "v".to_owned(),
            version_number: "1.0".to_owned(),
            version_type: "release".to_owned(),
            loaders: vec!["fabric".to_owned()],
            game_versions: vec!["26.3".to_owned()],
            date_published: "2026-10-21T00:00:00Z".parse().unwrap(),
            dependencies: Vec::new(),
            files: Vec::new(),
        };

        assert!(is_listed(&version));
    }
}
