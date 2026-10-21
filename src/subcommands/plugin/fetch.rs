use super::{CacheError, JAR_SUFFIX, SourceError};
use crate::command_line::PluginFetchArguments;
use crate::fetch_source::{FetchSource, LocalFile, UrlSource};
use crate::fetched_file_cache::{CachedFile, Fetched, Origin, Stored};
use crate::http_client::{self, HttpClient};
use crate::interruption::{self, Interrupted, Signal};
use crate::modrinth_api::response::{Version, VersionFile};
use crate::modrinth_api::selection::{self, Attribute, CandidateRules, Lazy, Request};
use crate::modrinth_source::ModrinthSource;
use crate::output::{self, Field};
use crate::plugin_compatibility::{self, Agreement, TargetServer};
use crate::prompt;
use crate::subcommands::modrinth_fetch;
use crate::subcommands::terminal_asker::TerminalAsker;

/// Every file of every version of a plugin is a candidate, narrowed by all five attributes
/// (plg-10).
const CANDIDATE_RULES: CandidateRules = CandidateRules {
    is_candidate,
    later_attributes: &[
        Attribute::Software,
        Attribute::MinecraftVersion,
        Attribute::ReleaseType,
        Attribute::FileName,
    ],
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Http(#[from] http_client::Error),
    #[error("Modrinth has no file of the plugin {identifier}")]
    NoFiles { identifier: String },
    #[error(transparent)]
    ModrinthFile(modrinth_fetch::Error),
    #[error(transparent)]
    Prompt(#[from] prompt::Error),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error(transparent)]
    Interrupted(#[from] Interrupted),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl From<modrinth_fetch::Error> for Error {
    fn from(error: modrinth_fetch::Error) -> Error {
        match error {
            modrinth_fetch::Error::Selection(selection::Error::NoFiles { identifier }) => {
                Error::NoFiles { identifier }
            }
            error => Error::ModrinthFile(error),
        }
    }
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::ModrinthFile(modrinth_fetch::Error::Modrinth(error)) => {
                super::modrinth_hint(error)
            }
            Error::ModrinthFile(modrinth_fetch::Error::Selection(
                selection::Error::NoMatchingVersion { identifier, .. },
            )) => Some(format!(
                "run `mism plugin versions modrinth:{identifier}` to see how each version matches \
                 the server, and give a source with a version number"
            )),
            Error::Source(error) => error.hint(),
            Error::Http(error) => error.hint(),
            Error::ModrinthFile(error) => error.hint(),
            Error::Prompt(error) => error.hint(),
            Error::Cache(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::NoFiles { .. } | Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Http(error) => error.interrupted_by(),
            Error::ModrinthFile(error) => error.interrupted_by(),
            Error::Prompt(error) => error.interrupted_by(),
            Error::Cache(error) => error.interrupted_by(),
            Error::Interrupted(Interrupted(signal)) => Some(*signal),
            Error::Source(_) | Error::NoFiles { .. } | Error::Output(_) => None,
        }
    }
}

/// `mism plugin fetch`: the fetch procedure, then the record of what it fetched (fetch-5).
pub fn run(arguments: &PluginFetchArguments) -> Result<(), Error> {
    let source = super::parse_source(&arguments.source)?;
    let cached_jar = fetch_jar(&arguments.source, &source, arguments.yes, None)?;
    write_fetched(&cached_jar)
}

/// The fetch procedure (取得の手順) from step 2, for `source`, which step 1 parsed from `text`.
/// With `yes`, nothing is asked (plg-11, fetch-2). With a `lazy` target server, a `modrinth:`
/// source chooses the newest version that matches it (inst-2); `install` gives one only for a
/// source without values.
pub fn fetch_jar(
    text: &str,
    source: &FetchSource,
    yes: bool,
    lazy: Option<TargetServer>,
) -> Result<CachedFile, Error> {
    let client = HttpClient::new()?;
    let fetched = match source {
        FetchSource::Modrinth(modrinth) => fetched_from_modrinth(&client, modrinth, lazy, yes)?,
        FetchSource::Url(url) => fetched_from_url(url, yes)?,
        FetchSource::File(local_jar) => fetched_local_jar(text, local_jar),
    };
    store(&client, &fetched)
}

/// Step 2 (plg-3, plg-5, plg-10 to plg-12, inst-2).
fn fetched_from_modrinth(
    client: &HttpClient,
    modrinth: &ModrinthSource,
    lazy: Option<TargetServer>,
    yes: bool,
) -> Result<Fetched, Error> {
    let Some(target) = lazy else {
        return file_to_fetch(client, modrinth, Request::Values(&modrinth.values), yes);
    };
    let matches = |version: &Version| matches_server(version, &target);
    let lazy = Lazy {
        matches: &matches,
        software: target.software,
        minecraft_version: target.minecraft_version,
    };
    file_to_fetch(client, modrinth, Request::Lazy(lazy), yes)
}

/// The file of a plugin that `request` chooses, asking unless `yes` (plg-10 to plg-12, inst-2).
fn file_to_fetch(
    client: &HttpClient,
    modrinth: &ModrinthSource,
    request: Request,
    yes: bool,
) -> Result<Fetched, Error> {
    Ok(modrinth_fetch::file_to_fetch(
        client,
        modrinth,
        &CANDIDATE_RULES,
        request,
        &mut TerminalAsker::new(yes),
    )?)
}

/// Whether a version of a plugin is for the server software and the Minecraft version of the
/// target server, the condition of `--lazy` (inst-2).
fn matches_server(version: &Version, target: &TargetServer) -> bool {
    plugin_compatibility::agreement(&version.loaders, &version.game_versions, target)
        == Agreement::Match
}

/// Every file of a plugin's Modrinth version is a candidate (plg-10).
fn is_candidate(_version: &Version, _file: &VersionFile) -> bool {
    true
}

/// Step 3 (fetch-2).
fn fetched_from_url(url: &UrlSource, yes: bool) -> Result<Fetched, Error> {
    let plugin_name = plugin_name_of_url(url, yes)?;
    Ok(Fetched {
        origin: Origin::Url(url.url().to_owned()),
        file_name: format!("{plugin_name}{JAR_SUFFIX}"),
        name: plugin_name,
        source: url.url().to_owned(),
        modrinth: None,
    })
}

/// The name made from the URL, which the user may replace. Asked before the download, also when
/// stdin is not a terminal; an empty answer keeps the made name (fetch-2).
fn plugin_name_of_url(url: &UrlSource, yes: bool) -> Result<String, Error> {
    let made = url.name(JAR_SUFFIX);
    output::write_to_stdout(&format!("plugin name from the URL: {made}"))?;
    if yes {
        return Ok(made);
    }
    let answer = prompt::ask(&format!("plugin name (empty for {made}): "))?;
    Ok(if answer.is_empty() { made } else { answer })
}

/// Step 4 (plg-5): the names of a local jar come from its path; its source is the text given.
fn fetched_local_jar(text: &str, local_jar: &LocalFile) -> Fetched {
    Fetched {
        origin: Origin::File(local_jar.path.clone()),
        name: local_jar.name.clone(),
        file_name: local_jar.file_name.clone(),
        source: text.to_owned(),
        modrinth: None,
    }
}

/// Steps 5 and 6: storing checks both names before it touches the cache directory (plg-6), then
/// stores the jar (fetch-1 to fetch-4).
fn store(client: &HttpClient, fetched: &Fetched) -> Result<CachedFile, Error> {
    // Storing may create the cache directory first.
    interruption::fail_if_received()?;
    match super::open_cache()?
        .store(client, fetched)
        .map_err(CacheError::from)?
    {
        Stored::New(cached_jar) => Ok(cached_jar),
        Stored::AlreadyCached(already_cached) => {
            let cached_jar = &already_cached.cached_file;
            output::write_to_stdout(&format!(
                "the jar is the same as the cached jar of {}: {}",
                cached_jar.name,
                cached_jar.path.display()
            ))?;
            Ok(already_cached.record_fetch().map_err(CacheError::from)?)
        }
    }
}

/// fetch-5: the plugin name, the version number, the source, the SHA-256, and the path.
fn write_fetched(cached_jar: &CachedFile) -> Result<(), Error> {
    let path = cached_jar.path.display().to_string();
    Ok(output::write_to_stdout(&output::record(&[
        Field::Value(Some(&cached_jar.name)),
        Field::Value(cached_jar.version_number()),
        Field::Value(Some(&cached_jar.source)),
        Field::Value(Some(&cached_jar.sha256)),
        Field::Value(Some(&path)),
    ]))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modrinth_api::response::FileHashes;
    use crate::modrinth_api::selection::{AskError, Asker};

    const TRUE_ENDING: &str =
        include_str!("../../../tests/fixtures/modrinth-versions-true-ending.json");

    /// The user cannot be asked (stdin is not a terminal, or `--yes`).
    struct CannotBeAsked;

    impl Asker for CannotBeAsked {
        fn ask(&mut self, _question: &str) -> Result<Option<String>, AskError> {
            Ok(None)
        }
    }

    /// plg-10: every file of every version of a plugin is a candidate.
    #[test]
    fn every_plugin_file_is_a_candidate() {
        let file = VersionFile {
            filename: "Example-sources.jar".to_owned(),
            url: "https://cdn.example/Example-sources.jar".to_owned(),
            hashes: FileHashes {
                sha512: "0".repeat(128),
            },
            file_type: Some("sources-jar".to_owned()),
        };
        let version = Version {
            id: "v".to_owned(),
            version_number: "1.0".to_owned(),
            version_type: "release".to_owned(),
            loaders: vec!["fabric".to_owned()],
            game_versions: vec!["26.3".to_owned()],
            date_published: "2026-10-21T00:00:00Z".parse().unwrap(),
            dependencies: Vec::new(),
            files: vec![file.clone()],
        };

        assert!(is_candidate(&version, &file));
    }

    /// plg-10: the plugin candidates of a number include every Modrinth version with it, the
    /// mod's too, so they are told apart by `software`.
    #[test]
    fn plugin_candidates_of_a_number_include_every_version() {
        let versions: Vec<Version> = serde_json::from_str(TRUE_ENDING).unwrap();
        let values = vec!["1.1.4".to_owned()];

        let error = selection::select(
            "true-ending",
            &versions,
            &CANDIDATE_RULES,
            Request::Values(&values),
            &mut CannotBeAsked,
        )
        .unwrap_err();

        assert!(
            matches!(
                &error,
                selection::Error::CannotAsk {
                    attribute: "software",
                    ..
                }
            ),
            "{error}"
        );
    }
}
