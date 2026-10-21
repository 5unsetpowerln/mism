use std::cmp::Reverse;
use std::collections::BTreeSet;

use super::CommonStepError;
use crate::command_line::PluginVersionsArguments;
use crate::compatibility::{self, TargetServer};
use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::modrinth_api::response::Version;
use crate::modrinth_api::{self, HttpModrinthApi, ModrinthApi};
use crate::output::{self, Field};
use crate::plugin_source;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Source(#[from] plugin_source::Error),
    #[error(transparent)]
    CommonStep(#[from] CommonStepError),
    #[error(transparent)]
    Http(#[from] http_client::Error),
    #[error(transparent)]
    Modrinth(#[from] modrinth_api::Error),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Source(error) => error.hint(),
            Error::CommonStep(error) => error.hint(),
            Error::Http(error) => error.hint(),
            Error::Modrinth(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::CommonStep(error) => error.interrupted_by(),
            Error::Http(error) => error.interrupted_by(),
            Error::Modrinth(error) => error.interrupted_by(),
            Error::Source(_) | Error::Output(_) => None,
        }
    }
}

/// `mism plugin versions` (vers-1 to vers-5). It writes the versions only after every request
/// succeeded (vers-5).
pub fn run(arguments: &PluginVersionsArguments) -> Result<(), Error> {
    let identifier = plugin_source::parse_versions_argument(&arguments.plugin)?;
    let server = super::target_server()?;
    let api = HttpModrinthApi::new(HttpClient::new()?)?;
    let project = modrinth_api::identified_project(&api, &identifier)?;
    let versions = api.project_versions(&project.id)?;
    let dependencies =
        modrinth_api::required_dependency_identifiers(&api, &versions.iter().collect::<Vec<_>>())?;
    let target = server.as_ref().map(super::target_of);
    write_versions(&newest_first(&versions, &dependencies), target.as_ref())
}

/// Step 6: each version with the identifiers of its required dependencies, newest first; versions
/// published at the same time keep Modrinth's order (vers-1).
fn newest_first<'a>(
    versions: &'a [Version],
    dependencies: &'a [BTreeSet<String>],
) -> Vec<(&'a Version, &'a BTreeSet<String>)> {
    let mut listed: Vec<_> = versions.iter().zip(dependencies).collect();
    // Stable, so equally new versions keep their order.
    listed.sort_by_key(|(version, _)| Reverse(version.date_published));
    listed
}

/// Step 7: one record per version (vers-1, vers-2, vers-3, plg-9).
fn write_versions(
    listed: &[(&Version, &BTreeSet<String>)],
    target: Option<&TargetServer>,
) -> Result<(), Error> {
    let lines: Vec<String> = listed
        .iter()
        .map(|(version, dependencies)| version_line(version, dependencies, target))
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

/// The version number, the release type, the server software, the newest Minecraft version, and
/// the required dependencies, then with a target server the agreement with it (vers-2, vers-3).
fn version_line(
    version: &Version,
    dependencies: &BTreeSet<String>,
    target: Option<&TargetServer>,
) -> String {
    let agreement = target.map(|target| {
        compatibility::agreement(&version.loaders, &version.game_versions, target).to_string()
    });
    let mut fields = vec![
        Field::Value(Some(&version.version_number)),
        Field::Value(Some(&version.version_type)),
        Field::List(version.loaders.iter().map(String::as_str).collect()),
        Field::Value(compatibility::newest_minecraft_version(
            &version.game_versions,
        )),
        Field::List(dependencies.iter().map(String::as_str).collect()),
    ];
    fields.extend(
        agreement
            .as_deref()
            .map(|agreement| Field::Value(Some(agreement))),
    );
    output::record(&fields)
}
