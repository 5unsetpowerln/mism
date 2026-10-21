use super::{CacheError, SourceError, ZIP_SUFFIX};
use crate::command_line::DatapackFetchArguments;
use crate::datapack_compatibility::{self, Agreement};
use crate::fetch_source::{FetchSource, LocalFile, UrlSource};
use crate::fetched_file_cache::{CachedFile, Fetched, Origin, Stored};
use crate::http_client::{self, HttpClient};
use crate::interruption::{self, Interrupted, Signal};
use crate::modrinth_api::response::{Version, VersionFile};
use crate::modrinth_api::selection::{self, Attribute, CandidateRules, Lazy, Request};
use crate::modrinth_source::ModrinthSource;
use crate::output::{self, Field};
use crate::prompt;
use crate::server_directory::ServerDirectory;
use crate::subcommands::modrinth_fetch;
use crate::subcommands::terminal_asker::TerminalAsker;

/// The datapack files of the datapack versions are the candidates, narrowed by the attributes
/// other than `software` (dp-5).
const CANDIDATE_RULES: CandidateRules = CandidateRules {
    is_candidate,
    later_attributes: &[
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
    #[error("Modrinth has no file of the datapack {identifier}")]
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
                "run `mism datapack versions modrinth:{identifier}` to see how each version \
                 matches the server, and give a source with a version number"
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

/// `mism datapack fetch`: the fetch procedure, then the record of what it fetched (dfetch-5).
pub fn run(arguments: &DatapackFetchArguments) -> Result<(), Error> {
    let source = super::parse_source(&arguments.source)?;
    let cached_file = fetch_file(&arguments.source, &source, arguments.yes, None)?;
    write_fetched(&cached_file)
}

/// The fetch procedure (取得の手順) from step 2, for `source`, which step 1 parsed from `text`.
/// With `yes`, nothing is asked (dp-5, dfetch-2). With a `lazy` target server, a `modrinth:`
/// source chooses the newest version that matches its Minecraft version (dinst-2); `install`
/// gives one only for a source without values.
pub fn fetch_file(
    text: &str,
    source: &FetchSource,
    yes: bool,
    lazy: Option<&ServerDirectory>,
) -> Result<CachedFile, Error> {
    let client = HttpClient::new()?;
    let fetched = match source {
        FetchSource::Modrinth(modrinth) => fetched_from_modrinth(&client, modrinth, lazy, yes)?,
        FetchSource::Url(url) => fetched_from_url(url, yes)?,
        FetchSource::File(local_file) => fetched_local_file(text, local_file),
    };
    store(&client, &fetched)
}

/// Step 2 (dp-4 to dp-6, dp-8, dinst-2).
fn fetched_from_modrinth(
    client: &HttpClient,
    modrinth: &ModrinthSource,
    lazy: Option<&ServerDirectory>,
    yes: bool,
) -> Result<Fetched, Error> {
    let Some(server) = lazy else {
        return file_to_fetch(client, modrinth, Request::Values(&modrinth.values), yes);
    };
    let minecraft_version = &server.mism_toml.minecraft_version;
    let matches = |version: &Version| matches_server(version, minecraft_version);
    let lazy = Lazy {
        matches: &matches,
        software: &server.mism_toml.software,
        minecraft_version,
    };
    file_to_fetch(client, modrinth, Request::Lazy(lazy), yes)
}

/// The datapack file that `request` chooses, asking unless `yes` (dp-5, dp-6, dinst-2).
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

/// Whether a datapack version supports the target server's Minecraft version, the condition of
/// `--lazy` (dinst-2).
fn matches_server(version: &Version, minecraft_version: &str) -> bool {
    datapack_compatibility::agreement(&version.game_versions, minecraft_version) == Agreement::Match
}

/// The files of a datapack version are candidates when Modrinth tells no kind of file for them,
/// which leaves out resource packs (dp-5).
fn is_candidate(version: &Version, file: &VersionFile) -> bool {
    super::is_datapack_version(version) && file.file_type.is_none()
}

/// Step 3 (dfetch-2).
fn fetched_from_url(url: &UrlSource, yes: bool) -> Result<Fetched, Error> {
    let datapack_name = datapack_name_of_url(url, yes)?;
    Ok(Fetched {
        origin: Origin::Url(url.url().to_owned()),
        file_name: format!("{datapack_name}{ZIP_SUFFIX}"),
        name: datapack_name,
        source: url.url().to_owned(),
        modrinth: None,
    })
}

/// The name made from the URL, which the user may replace. Asked before the download, also when
/// stdin is not a terminal; an empty answer keeps the made name (dfetch-2).
fn datapack_name_of_url(url: &UrlSource, yes: bool) -> Result<String, Error> {
    let made = url.name(ZIP_SUFFIX);
    output::write_to_stdout(&format!("datapack name from the URL: {made}"))?;
    if yes {
        return Ok(made);
    }
    let answer = prompt::ask(&format!("datapack name (empty for {made}): "))?;
    Ok(if answer.is_empty() { made } else { answer })
}

/// Step 4 (dp-8): the names of a local datapack file come from its path; its source is the text
/// given.
fn fetched_local_file(text: &str, local_file: &LocalFile) -> Fetched {
    Fetched {
        origin: Origin::File(local_file.path.clone()),
        name: local_file.name.clone(),
        file_name: local_file.file_name.clone(),
        source: text.to_owned(),
        modrinth: None,
    }
}

/// Steps 5 and 6: storing checks both names before it touches the cache directory (dp-9), then
/// stores the datapack file (dfetch-1 to dfetch-4).
fn store(client: &HttpClient, fetched: &Fetched) -> Result<CachedFile, Error> {
    // Storing may create the cache directory first.
    interruption::fail_if_received()?;
    match super::open_cache()?
        .store(client, fetched)
        .map_err(CacheError::from)?
    {
        Stored::New(cached_file) => Ok(cached_file),
        Stored::AlreadyCached(already_cached) => {
            let cached_file = &already_cached.cached_file;
            output::write_to_stdout(&format!(
                "the datapack is the same as the cached datapack of {}: {}",
                cached_file.name,
                cached_file.path.display()
            ))?;
            Ok(already_cached.record_fetch().map_err(CacheError::from)?)
        }
    }
}

/// dfetch-5: the datapack name, the version number, the source, the SHA-256, and the path.
fn write_fetched(cached_file: &CachedFile) -> Result<(), Error> {
    let path = cached_file.path.display().to_string();
    Ok(output::write_to_stdout(&output::record(&[
        Field::Value(Some(&cached_file.name)),
        Field::Value(cached_file.version_number()),
        Field::Value(Some(&cached_file.source)),
        Field::Value(Some(&cached_file.sha256)),
        Field::Value(Some(&path)),
    ]))?)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::modrinth_api::response::FileHashes;
    use crate::modrinth_api::selection::{AskError, Asker, Selection};

    const NAVAS_ZAMEGA: &str =
        include_str!("../../../tests/fixtures/modrinth-versions-navas-zamega.json");
    const TRUE_ENDING: &str =
        include_str!("../../../tests/fixtures/modrinth-versions-true-ending.json");

    fn saved(json: &str) -> Vec<Version> {
        serde_json::from_str(json).unwrap()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    /// A Modrinth version with these `loaders`, whose files have these names and `file_type`s.
    fn version_with_files(loaders: &[&str], files: &[(&str, Option<&str>)]) -> Version {
        Version {
            id: "v".to_owned(),
            version_number: "1.0".to_owned(),
            version_type: "release".to_owned(),
            loaders: strings(loaders),
            game_versions: strings(&["26.3"]),
            date_published: "2026-10-01T00:00:00Z".parse().unwrap(),
            dependencies: Vec::new(),
            files: files
                .iter()
                .map(|(name, file_type)| VersionFile {
                    filename: (*name).to_owned(),
                    url: format!("https://cdn.example/{name}"),
                    hashes: FileHashes {
                        sha512: "0".repeat(128),
                    },
                    file_type: file_type.map(str::to_owned),
                })
                .collect(),
        }
    }

    /// Fails the test if it is asked.
    struct NotAsked;

    impl Asker for NotAsked {
        fn ask(&mut self, question: &str) -> Result<Option<String>, AskError> {
            panic!("asked: {question}")
        }
    }

    /// Answers with the given answers in order and records the questions.
    struct Answering {
        answers: VecDeque<String>,
        questions: Vec<String>,
    }

    impl Asker for Answering {
        fn ask(&mut self, question: &str) -> Result<Option<String>, AskError> {
            self.questions.push(question.to_owned());
            Ok(Some(
                self.answers
                    .pop_front()
                    .expect("asked more often than the test answers"),
            ))
        }
    }

    fn select_with<'a>(
        versions: &'a [Version],
        values: &[&str],
        asker: &mut dyn Asker,
    ) -> Result<Selection<'a>, selection::Error> {
        let values = strings(values);
        selection::select(
            "example",
            versions,
            &CANDIDATE_RULES,
            Request::Values(&values),
            asker,
        )
    }

    /// `--lazy` on a Paper server of `minecraft_version`.
    fn select_lazily<'a>(
        versions: &'a [Version],
        minecraft_version: &str,
    ) -> Result<Selection<'a>, selection::Error> {
        let matches = |version: &Version| matches_server(version, minecraft_version);
        selection::select(
            "true-ending",
            versions,
            &CANDIDATE_RULES,
            Request::Lazy(Lazy {
                matches: &matches,
                software: "paper",
                minecraft_version,
            }),
            &mut NotAsked,
        )
    }

    /// dp-5: a datapack's candidates leave out files whose kind Modrinth tells.
    #[test]
    fn datapack_candidates_leave_out_resource_packs_and_mod_files() {
        let datapack = version_with_files(
            &["datapack"],
            &[
                ("Pack.zip", None),
                ("Resources.zip", Some("optional-resource-pack")),
            ],
        );
        let mod_version = version_with_files(&["fabric"], &[("Mod.jar", None)]);

        let candidates: Vec<_> = [
            (&datapack, &datapack.files[0]),
            (&datapack, &datapack.files[1]),
            (&mod_version, &mod_version.files[0]),
        ]
        .into_iter()
        .map(|(version, file)| is_candidate(version, file))
        .collect();

        assert_eq!(candidates, [true, false, false]);
    }

    /// d-select-5
    #[test]
    fn datapack_file_without_a_file_type_is_chosen_over_resource_packs() {
        let versions = saved(NAVAS_ZAMEGA);

        let selected = select_with(&versions, &["1.2.1"], &mut NotAsked).unwrap();

        assert_eq!(selected.file.filename, "ZA_ENfor1.7.zip");
    }

    /// d-select-6
    #[test]
    fn datapack_version_is_chosen_over_the_mod_version_with_the_same_number() {
        let versions = saved(TRUE_ENDING);

        let selected = select_with(&versions, &["1.1.4"], &mut NotAsked).unwrap();

        assert_eq!(selected.file.filename, "true-ending-v1.1.4-1.21.2-5.zip");
    }

    /// dp-5: the datapack versions with the same number differ in their Minecraft versions, so
    /// that is what is asked; `software` is not an attribute of datapacks.
    #[test]
    fn datapack_question_is_about_the_minecraft_version() {
        let versions = saved(TRUE_ENDING);
        let mut asker = Answering {
            answers: VecDeque::from(["1.21.9".to_owned()]),
            questions: Vec::new(),
        };

        let selected = select_with(&versions, &["1.1.4b"], &mut asker).unwrap();

        assert_eq!(
            (asker.questions.len(), selected.source),
            (1, "modrinth:example:1.1.4b:1.21.9".to_owned())
        );
    }

    /// dinst-2: `--lazy` chooses the newest datapack version whose Minecraft versions have the
    /// server's, whatever the server software.
    #[test]
    fn lazy_datapack_choice_is_the_newest_datapack_version_for_the_minecraft_version() {
        let versions = saved(TRUE_ENDING);

        let selection = select_lazily(&versions, "26.1.2").unwrap();

        assert_eq!(
            (selection.file.filename.as_str(), selection.source.as_str()),
            ("tru.e-ending-1.1.4d.zip", "modrinth:true-ending:1.1.4d")
        );
    }

    /// dinst-2: without a datapack version for the server's Minecraft version, `--lazy` is an
    /// error whose hint names `mism datapack versions`.
    #[test]
    fn lazy_datapack_choice_without_a_match_hints_at_the_datapack_versions() {
        let versions = saved(TRUE_ENDING);

        let error = Error::from(modrinth_fetch::Error::Selection(
            select_lazily(&versions, "26.3").unwrap_err(),
        ));

        assert_eq!(
            error.hint().as_deref(),
            Some(
                "run `mism datapack versions modrinth:true-ending` to see how each version \
                 matches the server, and give a source with a version number"
            )
        );
    }

    /// dp-5: a project without datapack files has no candidates.
    #[test]
    fn project_without_datapack_files_is_an_error() {
        let versions = [version_with_files(&["fabric"], &[("a.jar", None)])];

        assert!(matches!(
            select_with(&versions, &[], &mut NotAsked),
            Err(selection::Error::NoFiles { .. })
        ));
    }
}
