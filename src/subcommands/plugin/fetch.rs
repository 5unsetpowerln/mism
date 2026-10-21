use super::{CommonStepError, TerminalAsker};
use crate::command_line::FetchArguments;
use crate::compatibility::TargetServer;
use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::modrinth_api::selection::{self, Request};
use crate::modrinth_api::{self, HttpModrinthApi, ModrinthApi};
use crate::output::{self, Field};
use crate::plugin_cache::{self, CachedJar, Fetched, ModrinthRecord, Origin, Stored};
use crate::plugin_source::{self, LocalJar, ModrinthSource, PluginSource, UrlSource};
use crate::prompt;

const JAR_SUFFIX: &str = ".jar";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Source(#[from] plugin_source::Error),
    #[error(transparent)]
    Http(#[from] http_client::Error),
    #[error(transparent)]
    Modrinth(#[from] modrinth_api::Error),
    #[error(transparent)]
    Selection(#[from] selection::Error),
    #[error(transparent)]
    Prompt(#[from] prompt::Error),
    #[error(transparent)]
    PluginCache(#[from] plugin_cache::Error),
    #[error(transparent)]
    CommonStep(#[from] CommonStepError),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Source(error) => error.hint(),
            Error::Http(error) => error.hint(),
            Error::Modrinth(error) => error.hint(),
            Error::Selection(error) => error.hint(),
            Error::Prompt(error) => error.hint(),
            Error::PluginCache(error) => error.hint(),
            Error::CommonStep(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Http(error) => error.interrupted_by(),
            Error::Modrinth(error) => error.interrupted_by(),
            Error::Selection(error) => error.interrupted_by(),
            Error::Prompt(error) => error.interrupted_by(),
            Error::PluginCache(error) => error.interrupted_by(),
            Error::CommonStep(error) => error.interrupted_by(),
            Error::Source(_) | Error::Output(_) => None,
        }
    }
}

/// `mism plugin fetch`: the fetch procedure, then the record of what it fetched (fetch-5).
pub fn run(arguments: &FetchArguments) -> Result<(), Error> {
    let source = plugin_source::parse(&arguments.source)?;
    let cached_jar = fetch_jar(&arguments.source, &source, arguments.yes, None)?;
    write_fetched(&cached_jar)
}

/// The fetch procedure (取得の手順) from step 2, for `source`, which step 1 parsed from `text`.
/// With `yes`, nothing is asked (plg-11, fetch-2). With a `lazy` target server, a `modrinth:`
/// source chooses the newest version that matches it (inst-2); `install` gives one only for a
/// source without values.
pub fn fetch_jar(
    text: &str,
    source: &PluginSource,
    yes: bool,
    lazy: Option<TargetServer>,
) -> Result<CachedJar, Error> {
    let client = HttpClient::new()?;
    match source {
        PluginSource::Modrinth(modrinth) => fetch_from_modrinth(&client, modrinth, lazy, yes),
        PluginSource::Url(url) => fetch_from_url(&client, url, yes),
        PluginSource::File(local_jar) => fetch_local_jar(&client, text, local_jar),
    }
}

/// Step 2 (plg-3, plg-5, plg-10 to plg-12, inst-2).
fn fetch_from_modrinth(
    client: &HttpClient,
    modrinth: &ModrinthSource,
    lazy: Option<TargetServer>,
    yes: bool,
) -> Result<CachedJar, Error> {
    let api = HttpModrinthApi::new(client.clone())?;
    let project = modrinth_api::identified_project(&api, &modrinth.identifier)?;
    let versions = api.project_versions(&project.id)?;
    let request = match lazy {
        Some(target) => Request::Lazy(target),
        None => Request::Values(&modrinth.values),
    };
    let selected = selection::select(
        &modrinth.identifier,
        &versions,
        request,
        &mut TerminalAsker::new(yes),
    )?;
    let required_dependencies =
        modrinth_api::required_dependency_identifiers(&api, &[selected.version])?
            .pop()
            .expect("there is one set of identifiers for the one version");
    store(
        client,
        &Fetched {
            origin: Origin::Modrinth {
                url: &selected.file.url,
                sha512: &selected.file.hashes.sha512,
            },
            plugin_name: &project.title,
            file_name: &selected.file.filename,
            source: &selected.source,
            modrinth: Some(ModrinthRecord {
                loaders: selected.version.loaders.clone(),
                game_versions: selected.version.game_versions.clone(),
                required_dependencies: required_dependencies.into_iter().collect(),
            }),
        },
    )
}

/// Step 3 (fetch-2).
fn fetch_from_url(client: &HttpClient, url: &UrlSource, yes: bool) -> Result<CachedJar, Error> {
    let plugin_name = plugin_name_of_url(url, yes)?;
    let file_name = format!("{plugin_name}{JAR_SUFFIX}");
    store(
        client,
        &Fetched {
            origin: Origin::Url(url.url()),
            plugin_name: &plugin_name,
            file_name: &file_name,
            source: url.url(),
            modrinth: None,
        },
    )
}

/// The name made from the URL, which the user may replace. Asked before the download, also when
/// stdin is not a terminal; an empty answer keeps the made name (fetch-2).
fn plugin_name_of_url(url: &UrlSource, yes: bool) -> Result<String, Error> {
    let made = url.plugin_name();
    output::write_to_stdout(&format!("plugin name from the URL: {made}"))?;
    if yes {
        return Ok(made);
    }
    let answer = prompt::ask(&format!("plugin name (empty for {made}): "))?;
    Ok(if answer.is_empty() { made } else { answer })
}

/// Step 4 (plg-5): the names of a local jar come from its path; its source is the text given.
fn fetch_local_jar(
    client: &HttpClient,
    text: &str,
    local_jar: &LocalJar,
) -> Result<CachedJar, Error> {
    store(
        client,
        &Fetched {
            origin: Origin::File(&local_jar.path),
            plugin_name: &local_jar.plugin_name,
            file_name: &local_jar.jar_file_name,
            source: text,
            modrinth: None,
        },
    )
}

/// Steps 5 and 6: `plugin_cache::store` checks both names before it touches the cache directory
/// (plg-6), then stores the jar (fetch-1 to fetch-4).
fn store(client: &HttpClient, fetched: &Fetched) -> Result<CachedJar, Error> {
    // Storing may create the cache directory first.
    super::fail_if_interrupted()?;
    match plugin_cache::store(client, fetched)? {
        Stored::New(cached_jar) => Ok(cached_jar),
        Stored::AlreadyCached(already_cached) => {
            let cached_jar = &already_cached.cached_jar;
            output::write_to_stdout(&format!(
                "the jar is the same as the cached jar of {}: {}",
                cached_jar.name,
                cached_jar.path.display()
            ))?;
            Ok(already_cached.record_fetch()?)
        }
    }
}

/// fetch-5: the plugin name, the version number, the source, the SHA-256, and the path.
fn write_fetched(cached_jar: &CachedJar) -> Result<(), Error> {
    let path = cached_jar.path.display().to_string();
    Ok(output::write_to_stdout(&output::record(&[
        Field::Value(Some(&cached_jar.name)),
        Field::Value(plugin_source::version_number_of(&cached_jar.source)),
        Field::Value(Some(&cached_jar.source)),
        Field::Value(Some(&cached_jar.sha256)),
        Field::Value(Some(&path)),
    ]))?)
}
