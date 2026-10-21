use crate::fetched_file_cache::{Fetched, ModrinthRecord, Origin};
use crate::http_client::HttpClient;
use crate::interruption::Signal;
use crate::modrinth_api::selection::{self, Asker, CandidateRules, Request};
use crate::modrinth_api::{self, HttpModrinthApi, ModrinthApi};
use crate::modrinth_source::ModrinthSource;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Modrinth(#[from] modrinth_api::Error),
    #[error(transparent)]
    Selection(#[from] selection::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Modrinth(error) => error.hint(),
            Error::Selection(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Modrinth(error) => error.interrupted_by(),
            Error::Selection(error) => error.interrupted_by(),
        }
    }
}

/// Step 2 of the fetch procedure for a `modrinth:` source: the project named by its title, the
/// file that `request` chooses among the candidates of `rules` (asking through `asker`), and
/// Modrinth's statements about that file's Modrinth version (plg-3, plg-5, plg-10 to plg-12,
/// inst-2, dp-4 to dp-6, dp-8, dinst-2).
pub fn file_to_fetch(
    client: &HttpClient,
    source: &ModrinthSource,
    rules: &CandidateRules,
    request: Request,
    asker: &mut dyn Asker,
) -> Result<Fetched, Error> {
    let api = HttpModrinthApi::new(client.clone())?;
    let project = modrinth_api::identified_project(&api, &source.identifier)?;
    let versions = api.project_versions(&project.id)?;
    let selected = selection::select(&source.identifier, &versions, rules, request, asker)?;
    let required_dependencies =
        modrinth_api::required_dependency_identifiers(&api, &[selected.version])?
            .pop()
            .expect("there is one set of identifiers for the one version");
    Ok(Fetched {
        origin: Origin::Modrinth {
            url: selected.file.url.clone(),
            sha512: selected.file.hashes.sha512.clone(),
        },
        name: project.title,
        file_name: selected.file.filename.clone(),
        source: selected.source,
        modrinth: Some(ModrinthRecord {
            loaders: selected.version.loaders.clone(),
            game_versions: selected.version.game_versions.clone(),
            required_dependencies: required_dependencies.into_iter().collect(),
        }),
    })
}
