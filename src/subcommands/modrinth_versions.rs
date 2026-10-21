use std::cmp::Reverse;
use std::collections::BTreeSet;

use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::modrinth_api::response::Version;
use crate::modrinth_api::{self, HttpModrinthApi, ModrinthApi};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] http_client::Error),
    #[error(transparent)]
    Modrinth(#[from] modrinth_api::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Http(error) => error.hint(),
            Error::Modrinth(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Http(error) => error.interrupted_by(),
            Error::Modrinth(error) => error.interrupted_by(),
        }
    }
}

/// A Modrinth version to list, with the identifiers of its required dependencies (vers-2,
/// dvers-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub version: Version,
    pub required_dependencies: BTreeSet<String>,
}

/// Steps 3 to 6 of `versions`: the Modrinth versions of the project `identifier` that
/// `is_listed` accepts, newest first, each with its required dependencies (plg-3, vers-1, vers-2,
/// dp-4, dvers-1, dvers-2). Every request has succeeded when it returns (vers-5, dvers-5).
pub fn listed(identifier: &str, is_listed: fn(&Version) -> bool) -> Result<Vec<Listed>, Error> {
    let api = HttpModrinthApi::new(HttpClient::new()?)?;
    let project = modrinth_api::identified_project(&api, identifier)?;
    let versions: Vec<Version> = api
        .project_versions(&project.id)?
        .into_iter()
        .filter(is_listed)
        .collect();
    let dependencies =
        modrinth_api::required_dependency_identifiers(&api, &versions.iter().collect::<Vec<_>>())?;
    Ok(newest_first(versions, dependencies))
}

/// Step 6: published at the same time, versions keep Modrinth's order (vers-1, dvers-1).
fn newest_first(versions: Vec<Version>, dependencies: Vec<BTreeSet<String>>) -> Vec<Listed> {
    let mut listed: Vec<Listed> = versions
        .into_iter()
        .zip(dependencies)
        .map(|(version, required_dependencies)| Listed {
            version,
            required_dependencies,
        })
        .collect();
    // Stable, so equally new versions keep their order.
    listed.sort_by_key(|listed| Reverse(listed.version.date_published));
    listed
}
