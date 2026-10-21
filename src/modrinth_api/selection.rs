use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ptr;

use jiff::Timestamp;

use super::response::{Version, VersionFile};
use crate::compatibility::{self, Agreement, TargetServer};
use crate::interruption::Signal;
use crate::plugin_source;

/// The five attributes of a candidate file (plg-10), by the names errors and questions use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attribute {
    Version,
    Software,
    MinecraftVersion,
    ReleaseType,
    FileName,
}

impl Attribute {
    fn name(self) -> &'static str {
        match self {
            Attribute::Version => "version",
            Attribute::Software => "software",
            Attribute::MinecraftVersion => "minecraft_version",
            Attribute::ReleaseType => "release_type",
            Attribute::FileName => "file_name",
        }
    }
}

/// The attributes after `version`, in the order plg-10 narrows by them.
const LATER_ATTRIBUTES: [Attribute; 4] = [
    Attribute::Software,
    Attribute::MinecraftVersion,
    Attribute::ReleaseType,
    Attribute::FileName,
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Modrinth has no file of the plugin {identifier}")]
    NoFiles { identifier: String },
    #[error(
        "no file of {partial_source} has the {attribute} {value:?}; its files have these values of {attribute}: {}",
        .values.join(", ")
    )]
    NoFileWithValue {
        partial_source: String,
        attribute: &'static str,
        value: String,
        values: Vec<String>,
    },
    #[error(
        "{partial_source} has files with these values of {attribute}: {}",
        .values.join(", ")
    )]
    CannotAsk {
        partial_source: String,
        attribute: &'static str,
        values: Vec<String>,
    },
    #[error(
        "{answer:?} is not one of these values of {attribute}: {}",
        .values.join(", ")
    )]
    AnswerNotAValue {
        partial_source: String,
        attribute: &'static str,
        answer: String,
        values: Vec<String>,
    },
    #[error(
        "{selected_source} names one file, but these values are left over: {}",
        .values.join(", ")
    )]
    ValuesLeftOver {
        selected_source: String,
        values: Vec<String>,
    },
    #[error(
        "several files of {partial_source} have the same values of every attribute: {}",
        .urls.join(", ")
    )]
    Indistinguishable {
        partial_source: String,
        urls: Vec<String>,
    },
    #[error("the value {value:?} contains `:`, so no source can name it")]
    ValueWithColon { value: String },
    #[error("no version of {identifier} matches the server ({software} {minecraft_version})")]
    NoMatchingVersion {
        identifier: String,
        software: String,
        minecraft_version: String,
    },
    #[error("no source names only the file {url} that --lazy chose")]
    LazySourceAmbiguous { url: String },
    #[error("failed to ask for the value of {attribute}")]
    Ask {
        attribute: &'static str,
        #[source]
        source: AskError,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::NoFileWithValue {
                partial_source,
                values,
                ..
            }
            | Error::CannotAsk {
                partial_source,
                values,
                ..
            }
            | Error::AnswerNotAValue {
                partial_source,
                values,
                ..
            } => Some(format!(
                "add one of the values to the source: {}",
                values
                    .iter()
                    .map(|value| format!("`{partial_source}:{value}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Error::NoMatchingVersion { identifier, .. } => Some(format!(
                "run `mism plugin versions modrinth:{identifier}` to see how each version matches \
                 the server, and give a source with a version number"
            )),
            Error::NoFiles { .. }
            | Error::ValuesLeftOver { .. }
            | Error::Indistinguishable { .. }
            | Error::ValueWithColon { .. }
            | Error::LazySourceAmbiguous { .. }
            | Error::Ask { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Ask {
                source: AskError::Interrupted(signal),
                ..
            } => Some(*signal),
            _ => None,
        }
    }
}

/// Why asking the user failed.
#[derive(Debug, thiserror::Error)]
pub enum AskError {
    #[error("interrupted by {0}")]
    Interrupted(Signal),
    #[error(transparent)]
    Failed(Box<dyn std::error::Error + Send + Sync>),
}

/// Asks the user for a value; `subcommands/plugin/` implements it with stdin.
pub trait Asker {
    /// Writes `question` and reads one answer, or returns `None` without writing anything when
    /// the user cannot be asked (plg-11).
    fn ask(&mut self, question: &str) -> Result<Option<String>, AskError>;
}

/// How the file is chosen.
#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    /// By the attribute values the source gives, asking for what they leave open (plg-10,
    /// plg-11).
    Values(&'a [String]),
    /// For a source without values, from the newest version that matches the server (inst-2).
    Lazy(TargetServer<'a>),
}

/// The chosen file, its Modrinth version, and the source that names it (plg-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection<'a> {
    pub version: &'a Version,
    pub file: &'a VersionFile,
    pub source: String,
}

/// Narrows every file of `versions`, the version list of the plugin `identifier`, to one.
pub fn select<'a>(
    identifier: &str,
    versions: &'a [Version],
    request: Request,
    asker: &mut dyn Asker,
) -> Result<Selection<'a>, Error> {
    // Step 1.
    let all = all_candidates(versions);
    if all.is_empty() {
        return Err(Error::NoFiles {
            identifier: identifier.to_owned(),
        });
    }
    match request {
        Request::Values(values) => select_by_values(identifier, all, values, asker),
        Request::Lazy(target) => select_lazily(identifier, versions, &all, target, asker),
    }
}

fn select_by_values<'a>(
    identifier: &str,
    all: Vec<Candidate<'a>>,
    values: &[String],
    asker: &mut dyn Asker,
) -> Result<Selection<'a>, Error> {
    let mut narrowing = Narrowing::new(identifier, all, values, asker);
    let version = narrowing.version_value()?;
    narrowing.narrow(Attribute::Version, version)?;
    let chosen = narrowing.narrow_by_later_attributes()?;
    Ok(Selection {
        version: chosen.version,
        file: chosen.file,
        source: source_text(identifier, &narrowing.used)?,
    })
}

/// Steps 7 and 9.
fn select_lazily<'a>(
    identifier: &str,
    versions: &'a [Version],
    all: &[Candidate<'a>],
    target: TargetServer,
    asker: &mut dyn Asker,
) -> Result<Selection<'a>, Error> {
    let version = newest_matching_version(identifier, versions, &target)?;
    let files = files_of(version);
    let mut narrowing = Narrowing::new(identifier, files, &[], asker);
    narrowing.used.push(version.version_number.clone());
    let chosen = narrowing.narrow_by_later_attributes()?;
    let source = lazy_source(identifier, all, chosen, &narrowing.decided, &target)?;
    Ok(Selection {
        version: chosen.version,
        file: chosen.file,
        source,
    })
}

/// A file of a Modrinth version, with the attributes it takes from both.
#[derive(Debug, Clone, Copy)]
struct Candidate<'a> {
    version: &'a Version,
    file: &'a VersionFile,
}

impl<'a> Candidate<'a> {
    /// The value of the attribute; for `software` and `minecraft_version`, the list's elements.
    fn values(self, attribute: Attribute) -> Vec<&'a str> {
        let list = |list: &'a [String]| list.iter().map(String::as_str).collect();
        match attribute {
            Attribute::Version => vec![&self.version.version_number],
            Attribute::Software => list(&self.version.loaders),
            Attribute::MinecraftVersion => list(&self.version.game_versions),
            Attribute::ReleaseType => vec![&self.version.version_type],
            Attribute::FileName => vec![&self.file.filename],
        }
    }

    /// Whether the attribute is `value`, or for a list attribute contains it.
    fn has(self, attribute: Attribute, value: &str) -> bool {
        self.values(attribute).contains(&value)
    }

    fn is(self, other: Candidate) -> bool {
        ptr::eq(self.file, other.file)
    }
}

fn all_candidates(versions: &[Version]) -> Vec<Candidate<'_>> {
    versions.iter().flat_map(files_of).collect()
}

fn files_of(version: &Version) -> Vec<Candidate<'_>> {
    version
        .files
        .iter()
        .map(|file| Candidate { version, file })
        .collect()
}

/// The number of kinds of values the candidates have; a list attribute counts whole lists.
fn kinds(candidates: &[Candidate], attribute: Attribute) -> usize {
    candidates
        .iter()
        .map(|candidate| candidate.values(attribute))
        .collect::<BTreeSet<_>>()
        .len()
}

/// The values to show for the attribute, each once: version numbers newest first by the newest
/// version with that number, then by string; other values by string (plg-11).
fn shown_values(candidates: &[Candidate], attribute: Attribute) -> Vec<String> {
    if attribute != Attribute::Version {
        let values: BTreeSet<&str> = candidates
            .iter()
            .flat_map(|candidate| candidate.values(attribute))
            .collect();
        return values.into_iter().map(str::to_owned).collect();
    }
    let mut newest: BTreeMap<&str, Timestamp> = BTreeMap::new();
    for candidate in candidates {
        let published = candidate.version.date_published;
        newest
            .entry(&candidate.version.version_number)
            .and_modify(|date| *date = (*date).max(published))
            .or_insert(published);
    }
    let mut numbers: Vec<(&str, Timestamp)> = newest.into_iter().collect();
    // Stable, so numbers published at the same time stay in string order.
    numbers.sort_by_key(|(_, published)| Reverse(*published));
    numbers
        .into_iter()
        .map(|(number, _)| number.to_owned())
        .collect()
}

/// The candidates while they are narrowed, the values given and used, and how to ask.
struct Narrowing<'a, 'b> {
    identifier: &'b str,
    candidates: Vec<Candidate<'a>>,
    given: VecDeque<String>,
    /// The values for the source, in the order they were used (plg-12).
    used: Vec<String>,
    /// The attributes narrowed by and their values.
    decided: Vec<(Attribute, String)>,
    asker: &'b mut dyn Asker,
}

impl<'a, 'b> Narrowing<'a, 'b> {
    fn new(
        identifier: &'b str,
        candidates: Vec<Candidate<'a>>,
        given: &[String],
        asker: &'b mut dyn Asker,
    ) -> Narrowing<'a, 'b> {
        Narrowing {
            identifier,
            candidates,
            given: given.iter().cloned().collect(),
            used: Vec::new(),
            decided: Vec::new(),
            asker,
        }
    }

    /// Step 2: the first given value, the only version number, or the user's answer.
    fn version_value(&mut self) -> Result<String, Error> {
        if let Some(value) = self.given.pop_front() {
            return Ok(value);
        }
        if kinds(&self.candidates, Attribute::Version) == 1 {
            return Ok(self.candidates[0].version.version_number.clone());
        }
        self.ask(Attribute::Version)
    }

    /// Step 3.
    fn narrow(&mut self, attribute: Attribute, value: String) -> Result<(), Error> {
        let remaining: Vec<Candidate> = self
            .candidates
            .iter()
            .copied()
            .filter(|candidate| candidate.has(attribute, &value))
            .collect();
        if remaining.is_empty() {
            return Err(Error::NoFileWithValue {
                partial_source: self.partial_source(),
                attribute: attribute.name(),
                value,
                values: shown_values(&self.candidates, attribute),
            });
        }
        self.candidates = remaining;
        self.decided.push((attribute, value.clone()));
        self.used.push(value);
        Ok(())
    }

    /// Steps 4 and 5.
    fn narrow_by_later_attributes(&mut self) -> Result<Candidate<'a>, Error> {
        for attribute in LATER_ATTRIBUTES {
            if self.candidates.len() == 1 {
                break;
            }
            if kinds(&self.candidates, attribute) == 1 {
                continue;
            }
            let value = match self.given.pop_front() {
                Some(value) => value,
                None => self.ask(attribute)?,
            };
            self.narrow(attribute, value)?;
        }
        self.only_candidate()
    }

    /// Step 4, and the end of step 5.
    fn only_candidate(&self) -> Result<Candidate<'a>, Error> {
        match self.candidates.as_slice() {
            [candidate] if self.given.is_empty() => Ok(*candidate),
            [_] => Err(Error::ValuesLeftOver {
                selected_source: self.partial_source(),
                values: self.given.iter().cloned().collect(),
            }),
            candidates => Err(Error::Indistinguishable {
                partial_source: self.partial_source(),
                urls: candidates
                    .iter()
                    .map(|candidate| candidate.file.url.clone())
                    .collect(),
            }),
        }
    }

    /// Step 6.
    fn ask(&mut self, attribute: Attribute) -> Result<String, Error> {
        let name = attribute.name();
        let partial_source = self.partial_source();
        let values = shown_values(&self.candidates, attribute);
        let question = format!(
            "{partial_source} has files with these values of {name}:\n{}\n{name}: ",
            values.join("\n")
        );
        let answer = self.asker.ask(&question).map_err(|source| Error::Ask {
            attribute: name,
            source,
        })?;
        match answer {
            None => Err(Error::CannotAsk {
                partial_source,
                attribute: name,
                values,
            }),
            Some(answer) if values.contains(&answer) => Ok(answer),
            Some(answer) => Err(Error::AnswerNotAValue {
                partial_source,
                attribute: name,
                answer,
                values,
            }),
        }
    }

    /// `modrinth:`, the identifier, and the values used so far.
    fn partial_source(&self) -> String {
        let values: Vec<&str> = self.used.iter().map(String::as_str).collect();
        plugin_source::modrinth_source(self.identifier, &values)
    }
}

/// Step 8: the source of the chosen file; a value with `:` cannot be written in it.
fn source_text(identifier: &str, used: &[String]) -> Result<String, Error> {
    if let Some(value) = used.iter().find(|value| value.contains(':')) {
        return Err(Error::ValueWithColon {
            value: value.clone(),
        });
    }
    let values: Vec<&str> = used.iter().map(String::as_str).collect();
    Ok(plugin_source::modrinth_source(identifier, &values))
}

/// Step 7: of the versions with files that match the server, the newest; of equally new ones,
/// the first in the list.
fn newest_matching_version<'a>(
    identifier: &str,
    versions: &'a [Version],
    target: &TargetServer,
) -> Result<&'a Version, Error> {
    versions
        .iter()
        .enumerate()
        .filter(|(_, version)| {
            !version.files.is_empty()
                && compatibility::agreement(&version.loaders, &version.game_versions, target)
                    == Agreement::Match
        })
        .max_by_key(|(index, version)| (version.date_published, Reverse(*index)))
        .map(|(_, version)| version)
        .ok_or_else(|| Error::NoMatchingVersion {
            identifier: identifier.to_owned(),
            software: target.software.to_owned(),
            minecraft_version: target.minecraft_version.to_owned(),
        })
}

/// Step 9: narrows every file again without asking, by the chosen file's values and the server,
/// so that the source names the chosen file without `--lazy` (plg-12).
fn lazy_source(
    identifier: &str,
    all: &[Candidate],
    chosen: Candidate,
    decided: &[(Attribute, String)],
    target: &TargetServer,
) -> Result<String, Error> {
    let mut candidates = all.to_vec();
    let mut used = Vec::new();
    for attribute in [Attribute::Version].into_iter().chain(LATER_ATTRIBUTES) {
        if attribute != Attribute::Version
            && (candidates.len() == 1 || kinds(&candidates, attribute) == 1)
        {
            continue;
        }
        let value = lazy_value(attribute, chosen, decided, target);
        candidates.retain(|candidate| candidate.has(attribute, &value));
        used.push(value);
    }
    match candidates.as_slice() {
        [only] if only.is(chosen) => source_text(identifier, &used),
        _ => Err(Error::LazySourceAmbiguous {
            url: chosen.file.url.clone(),
        }),
    }
}

fn lazy_value(
    attribute: Attribute,
    chosen: Candidate,
    decided: &[(Attribute, String)],
    target: &TargetServer,
) -> String {
    let decided_or = |server_value: &str| {
        decided
            .iter()
            .find(|(decided_attribute, _)| *decided_attribute == attribute)
            .map_or_else(|| server_value.to_owned(), |(_, value)| value.clone())
    };
    match attribute {
        Attribute::Version => chosen.version.version_number.clone(),
        Attribute::Software => decided_or(target.software),
        Attribute::MinecraftVersion => decided_or(target.minecraft_version),
        Attribute::ReleaseType => chosen.version.version_type.clone(),
        Attribute::FileName => chosen.file.filename.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::super::response::{FileHashes, Version, VersionFile};
    use super::*;

    const FASTASYNCWORLDEDIT: &str =
        include_str!("../../tests/fixtures/modrinth-versions-fastasyncworldedit.json");
    const LUCKPERMS: &str = include_str!("../../tests/fixtures/modrinth-versions-luckperms.json");
    const PAPER_26_3: TargetServer = TargetServer {
        software: "paper",
        minecraft_version: "26.3",
    };

    fn saved(json: &str) -> Vec<Version> {
        serde_json::from_str(json).unwrap()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    /// A Modrinth version of the release type `release` with a file per name.
    fn version(
        number: &str,
        loaders: &[&str],
        game_versions: &[&str],
        published: &str,
        file_names: &[&str],
    ) -> Version {
        Version {
            id: format!("{number}-{}", loaders.join("-")),
            version_number: number.to_owned(),
            version_type: "release".to_owned(),
            loaders: strings(loaders),
            game_versions: strings(game_versions),
            date_published: published.parse().unwrap(),
            dependencies: Vec::new(),
            files: file_names
                .iter()
                .map(|name| VersionFile {
                    filename: (*name).to_owned(),
                    url: format!("https://cdn.example/{number}/{name}"),
                    hashes: FileHashes {
                        sha512: "0".repeat(128),
                    },
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

    /// The user cannot be asked (stdin is not a terminal, or `--yes`).
    struct CannotBeAsked;

    impl Asker for CannotBeAsked {
        fn ask(&mut self, _question: &str) -> Result<Option<String>, AskError> {
            Ok(None)
        }
    }

    /// Answers with the given answers in order and records the questions.
    struct Answering {
        answers: VecDeque<String>,
        questions: Vec<String>,
    }

    impl Answering {
        fn with(answers: &[&str]) -> Answering {
            Answering {
                answers: answers.iter().map(|answer| (*answer).to_owned()).collect(),
                questions: Vec::new(),
            }
        }
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

    fn select_with(
        versions: &[Version],
        values: &[&str],
        asker: &mut dyn Asker,
    ) -> Result<(String, String), Error> {
        let values = strings(values);
        select("example", versions, Request::Values(&values), asker)
            .map(|selection| (selection.file.filename.clone(), selection.source))
    }

    /// plg-10, plg-12: a version number selects the file of that version.
    #[test]
    fn version_number_selects_the_file_of_that_version() {
        let versions = saved(LUCKPERMS);
        let values = strings(&["v5.5.71-bukkit"]);

        let selection = select(
            "luckperms",
            &versions,
            Request::Values(&values),
            &mut NotAsked,
        )
        .unwrap();

        assert_eq!(
            (selection.file.filename.as_str(), selection.source.as_str()),
            (
                "LuckPerms-Bukkit-5.5.71.jar",
                "modrinth:luckperms:v5.5.71-bukkit"
            )
        );
    }

    /// plg-10: without values, the only version number is used without asking.
    #[test]
    fn only_version_number_is_used_without_asking() {
        let versions = [version(
            "1.0",
            &["paper"],
            &["26.3"],
            "2026-10-01T00:00:00Z",
            &["a.jar"],
        )];

        assert_eq!(
            select_with(&versions, &[], &mut NotAsked).unwrap(),
            ("a.jar".to_owned(), "modrinth:example:1.0".to_owned())
        );
    }

    /// plg-11: when the user cannot be asked, the error names the attribute and the values,
    /// version numbers newest first.
    #[test]
    fn version_that_cannot_be_asked_is_an_error_with_the_values() {
        let versions = [
            version(
                "1.0",
                &["paper"],
                &["26.3"],
                "2026-10-01T00:00:00Z",
                &["a.jar"],
            ),
            version(
                "2.0",
                &["paper"],
                &["26.3"],
                "2026-10-02T00:00:00Z",
                &["b.jar"],
            ),
        ];

        let error = select_with(&versions, &[], &mut CannotBeAsked).unwrap_err();

        assert!(
            matches!(&error, Error::CannotAsk { attribute: "version", values, .. }
                if *values == strings(&["2.0", "1.0"])),
            "{error}"
        );
    }

    /// plg-11: a version number shown is ordered by the newest version that has it.
    #[test]
    fn version_number_is_ordered_by_its_newest_version() {
        let versions = [
            version(
                "2.0",
                &["paper"],
                &["26.3"],
                "2026-10-01T00:00:00Z",
                &["a.jar"],
            ),
            version(
                "3.0",
                &["paper"],
                &["26.3"],
                "2026-10-02T00:00:00Z",
                &["b.jar"],
            ),
            version(
                "2.0",
                &["spigot"],
                &["26.3"],
                "2026-10-03T00:00:00Z",
                &["c.jar"],
            ),
        ];

        let error = select_with(&versions, &[], &mut CannotBeAsked).unwrap_err();

        assert!(
            matches!(&error, Error::CannotAsk { values, .. } if *values == strings(&["2.0", "3.0"])),
            "{error}"
        );
    }

    /// plg-11: the question names the source so far, the attribute, and the values.
    #[test]
    fn question_shows_the_source_so_far_and_the_values() {
        let versions = saved(FASTASYNCWORLDEDIT);
        let mut asker = Answering::with(&["paper"]);

        select_with(&versions, &["2.16.1"], &mut asker).unwrap();

        assert_eq!(
            asker.questions,
            [
                "modrinth:example:2.16.1 has files with these values of software:\npaper\nspigot\nsoftware: "
            ]
        );
    }

    /// plg-11, plg-12: an answer narrows the files and is written into the source.
    #[test]
    fn answer_narrows_the_files_and_is_in_the_source() {
        let versions = saved(FASTASYNCWORLDEDIT);

        let selected = select_with(&versions, &["2.16.1"], &mut Answering::with(&["paper"]));

        assert_eq!(
            selected.unwrap(),
            (
                "FastAsyncWorldEdit-Paper-2.16.1.jar".to_owned(),
                "modrinth:example:2.16.1:paper".to_owned()
            )
        );
    }

    /// plg-11: an answer that is not one of the values is an error.
    #[test]
    fn answer_that_is_not_a_value_is_an_error() {
        let versions = saved(FASTASYNCWORLDEDIT);

        let result = select_with(&versions, &["2.16.1"], &mut Answering::with(&["bukkit"]));

        assert!(matches!(result, Err(Error::AnswerNotAValue { .. })));
    }

    /// plg-11: an empty answer is an error.
    #[test]
    fn empty_answer_is_an_error() {
        let versions = saved(FASTASYNCWORLDEDIT);

        let result = select_with(&versions, &["2.16.1"], &mut Answering::with(&[""]));

        assert!(matches!(result, Err(Error::AnswerNotAValue { .. })));
    }

    /// plg-10: lists are counted as whole lists, so two files of one version are told apart by
    /// their names only.
    #[test]
    fn files_of_one_version_are_told_apart_by_their_names() {
        let versions = saved(FASTASYNCWORLDEDIT);

        let error = select_with(&versions, &["2.14.1"], &mut CannotBeAsked).unwrap_err();

        assert!(
            matches!(
                &error,
                Error::CannotAsk {
                    attribute: "file_name",
                    ..
                }
            ),
            "{error}"
        );
    }

    /// plg-10: a given value that no file has is an error with the attribute and the values.
    #[test]
    fn value_that_no_file_has_is_an_error_with_the_values() {
        let versions = saved(FASTASYNCWORLDEDIT);

        let error = select_with(&versions, &["2.16.1", "bukkit"], &mut NotAsked).unwrap_err();

        assert!(
            matches!(&error, Error::NoFileWithValue { attribute: "software", values, .. }
                if *values == strings(&["paper", "spigot"])),
            "{error}"
        );
    }

    /// plg-10, plg-11: the hint offers the source with each value added.
    #[test]
    fn hint_offers_the_source_with_each_value() {
        let versions = saved(FASTASYNCWORLDEDIT);

        let error = select_with(&versions, &["2.16.1"], &mut CannotBeAsked).unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some(
                "add one of the values to the source: `modrinth:example:2.16.1:paper`, \
                 `modrinth:example:2.16.1:spigot`"
            )
        );
    }

    /// plg-10: values left after one file remains are an error.
    #[test]
    fn values_left_after_one_file_are_an_error() {
        let versions = saved(LUCKPERMS);

        let result = select_with(&versions, &["v5.5.71-bukkit", "paper"], &mut NotAsked);

        assert!(matches!(result, Err(Error::ValuesLeftOver { .. })));
    }

    /// plg-12: an attribute skipped because all files share its value is not in the source.
    #[test]
    fn skipped_attributes_are_not_in_the_source() {
        let versions = [
            version(
                "1.0",
                &["paper"],
                &["26.3"],
                "2026-10-01T00:00:00Z",
                &["a.jar"],
            ),
            version(
                "1.0",
                &["paper"],
                &["26.2"],
                "2026-10-02T00:00:00Z",
                &["b.jar"],
            ),
        ];

        assert_eq!(
            select_with(&versions, &["1.0", "26.2"], &mut NotAsked)
                .unwrap()
                .1,
            "modrinth:example:1.0:26.2"
        );
    }

    /// d-select-1: the source of the file chosen with `--lazy` on a Paper 26.3 server.
    #[test]
    fn lazy_choice_has_a_source_with_the_values_that_tell_it_apart() {
        let versions = saved(FASTASYNCWORLDEDIT);

        let selection = select(
            "fastasyncworldedit",
            &versions,
            Request::Lazy(PAPER_26_3),
            &mut NotAsked,
        )
        .unwrap();

        assert_eq!(selection.source, "modrinth:fastasyncworldedit:2.16.1:paper");
    }

    /// d-select-1: the source from `--lazy` chooses the same file without `--lazy` or asking.
    #[test]
    fn source_from_a_lazy_choice_chooses_the_same_file() {
        let versions = saved(FASTASYNCWORLDEDIT);
        let lazy = select(
            "fastasyncworldedit",
            &versions,
            Request::Lazy(PAPER_26_3),
            &mut NotAsked,
        )
        .unwrap();
        let values: Vec<String> = lazy.source.split(':').skip(2).map(str::to_owned).collect();

        let again = select(
            "fastasyncworldedit",
            &versions,
            Request::Values(&values),
            &mut NotAsked,
        )
        .unwrap();

        assert!(ptr::eq(again.file, lazy.file));
    }

    /// d-select-2
    #[test]
    fn value_with_a_colon_is_an_error() {
        let versions = [version(
            "1.0:beta",
            &["paper"],
            &["26.3"],
            "2026-10-01T00:00:00Z",
            &["a.jar"],
        )];

        assert!(matches!(
            select_with(&versions, &[], &mut NotAsked),
            Err(Error::ValueWithColon { .. })
        ));
    }

    /// d-select-3
    #[test]
    fn files_that_no_attribute_tells_apart_are_an_error() {
        let versions = [
            version(
                "1.0",
                &["paper"],
                &["26.3"],
                "2026-10-01T00:00:00Z",
                &["a.jar"],
            ),
            version(
                "1.0",
                &["paper"],
                &["26.3"],
                "2026-10-02T00:00:00Z",
                &["a.jar"],
            ),
        ];

        assert!(matches!(
            select_with(&versions, &["1.0"], &mut NotAsked),
            Err(Error::Indistinguishable { urls, .. }) if urls.len() == 2
        ));
    }

    /// d-select-4
    #[test]
    fn versions_without_files_are_an_error() {
        let versions = [version(
            "1.0",
            &["paper"],
            &["26.3"],
            "2026-10-01T00:00:00Z",
            &[],
        )];

        assert!(matches!(
            select_with(&versions, &["1.0"], &mut NotAsked),
            Err(Error::NoFiles { .. })
        ));
    }

    /// inst-2: without a matching version `--lazy` chooses nothing else.
    #[test]
    fn lazy_choice_without_a_matching_version_is_an_error() {
        let versions = [version(
            "1.0",
            &["paper"],
            &["26.2"],
            "2026-10-01T00:00:00Z",
            &["a.jar"],
        )];

        assert!(matches!(
            select(
                "example",
                &versions,
                Request::Lazy(PAPER_26_3),
                &mut NotAsked
            ),
            Err(Error::NoMatchingVersion { .. })
        ));
    }

    /// inst-2: `--lazy` chooses the newest matching version, the first of equally new ones.
    #[test]
    fn lazy_choice_is_the_newest_matching_version_and_the_first_of_equals() {
        let versions = [
            version(
                "1.0",
                &["paper"],
                &["26.3"],
                "2026-10-01T00:00:00Z",
                &["old.jar"],
            ),
            version(
                "2.0",
                &["paper"],
                &["26.3"],
                "2026-10-02T00:00:00Z",
                &["first.jar"],
            ),
            version(
                "2.1",
                &["paper"],
                &["26.3"],
                "2026-10-02T00:00:00Z",
                &["second.jar"],
            ),
            version(
                "3.0",
                &["spigot"],
                &["26.3"],
                "2026-10-03T00:00:00Z",
                &["spigot.jar"],
            ),
        ];

        let selection = select(
            "example",
            &versions,
            Request::Lazy(PAPER_26_3),
            &mut NotAsked,
        )
        .unwrap();

        assert_eq!(selection.file.filename, "first.jar");
    }

    /// d-prompt-1 through selection: an interruption while asking is reported as one.
    #[test]
    fn interruption_while_asking_is_an_interruption() {
        struct Interrupted;
        impl Asker for Interrupted {
            fn ask(&mut self, _question: &str) -> Result<Option<String>, AskError> {
                Err(AskError::Interrupted(Signal::Interrupt))
            }
        }
        let versions = saved(FASTASYNCWORLDEDIT);

        let error = select_with(&versions, &["2.16.1"], &mut Interrupted).unwrap_err();

        assert_eq!(error.interrupted_by(), Some(Signal::Interrupt));
    }
}
