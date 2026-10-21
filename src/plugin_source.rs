use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::file_name;

const MODRINTH_PREFIX: &str = "modrinth:";
const URL_PREFIXES: [&str; 2] = ["https://", "http://"];
const JAR_SUFFIX: &str = ".jar";
/// The characters of a Modrinth identifier besides ASCII letters and digits.
const IDENTIFIER_PUNCTUATION: &str = "_!@$()`.+,\"-'";
const IDENTIFIER_LENGTHS: std::ops::RangeInclusive<usize> = 3..=64;
const THREE_FORMS: &str = "give `modrinth:<identifier>` (optionally followed by `:<value>`s), a \
                           jar URL starting with `https://` or `http://`, or the path of a local \
                           jar file";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "{identifier:?} is not a Modrinth identifier: it must be 3 to 64 characters, each an ASCII \
         letter or digit or one of {IDENTIFIER_PUNCTUATION}"
    )]
    InvalidIdentifier { identifier: String },
    #[error("{text:?} is not `modrinth:` followed by an identifier and nothing else")]
    NotModrinthProject { text: String },
    #[error("{} is not a source: failed to examine it as a local jar file", path.display())]
    ExamineLocalJar {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a source: it is not a regular file", path.display())]
    NotRegularFile { path: PathBuf },
    #[error("{} is not a source: its name does not end with `.jar`", path.display())]
    NotJarName { path: PathBuf },
    #[error(transparent)]
    FileName(#[from] file_name::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::InvalidIdentifier { .. } => {
                Some("run `mism plugin search <term>` to find the identifier".to_owned())
            }
            Error::ExamineLocalJar { .. }
            | Error::NotRegularFile { .. }
            | Error::NotJarName { .. } => Some(THREE_FORMS.to_owned()),
            Error::FileName(error) => error.hint(),
            Error::NotModrinthProject { .. } => None,
        }
    }
}

/// A <取得元の指定> (plg-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    Modrinth(ModrinthSource),
    Url(UrlSource),
    File(LocalJar),
}

/// `modrinth:<identifier>` and the attribute values after it, in the order given (plg-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModrinthSource {
    pub identifier: String,
    pub values: Vec<String>,
}

/// A URL starting with `https://` or `http://`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlSource {
    url: String,
}

/// A local jar file that exists, with the names plg-5 gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalJar {
    /// As given; a relative path is relative to the current directory.
    pub path: PathBuf,
    pub plugin_name: String,
    pub jar_file_name: String,
}

/// Parses a <取得元の指定> by its beginning (plg-3).
pub fn parse(text: &str) -> Result<PluginSource, Error> {
    if let Some(rest) = text.strip_prefix(MODRINTH_PREFIX) {
        return parse_modrinth(rest).map(PluginSource::Modrinth);
    }
    if URL_PREFIXES.iter().any(|prefix| text.starts_with(prefix)) {
        return Ok(PluginSource::Url(UrlSource {
            url: text.to_owned(),
        }));
    }
    parse_local_jar(Path::new(text)).map(PluginSource::File)
}

fn parse_modrinth(rest: &str) -> Result<ModrinthSource, Error> {
    let mut elements = rest.split(':');
    let identifier = elements
        .next()
        .expect("splitting yields at least one element");
    check_identifier(identifier)?;
    Ok(ModrinthSource {
        identifier: identifier.to_owned(),
        values: elements.map(str::to_owned).collect(),
    })
}

/// The identifier in the argument of `mism plugin versions`, which is `modrinth:` and an
/// identifier only (vers-4).
pub fn parse_versions_argument(text: &str) -> Result<String, Error> {
    let not_modrinth_project = || Error::NotModrinthProject {
        text: text.to_owned(),
    };
    let identifier = text
        .strip_prefix(MODRINTH_PREFIX)
        .ok_or_else(not_modrinth_project)?;
    if identifier.contains(':') {
        return Err(not_modrinth_project());
    }
    check_identifier(identifier)?;
    Ok(identifier.to_owned())
}

/// Checks the identifier against the slug characters of Modrinth's documentation, so that no
/// identifier changes the endpoint it is put in (plg-3).
fn check_identifier(identifier: &str) -> Result<(), Error> {
    let valid = IDENTIFIER_LENGTHS.contains(&identifier.chars().count())
        && identifier.chars().all(|character| {
            character.is_ascii_alphanumeric() || IDENTIFIER_PUNCTUATION.contains(character)
        });
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidIdentifier {
            identifier: identifier.to_owned(),
        })
    }
}

fn parse_local_jar(path: &Path) -> Result<LocalJar, Error> {
    check_regular_file(path)?;
    let jar_file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.ends_with(JAR_SUFFIX))
        .ok_or_else(|| Error::NotJarName {
            path: path.to_path_buf(),
        })?;
    let plugin_name = jar_file_name
        .strip_suffix(JAR_SUFFIX)
        .expect("the name ends with `.jar`");
    file_name::check_plugin_name(plugin_name)?;
    file_name::check_jar_file_name(jar_file_name)?;
    Ok(LocalJar {
        path: path.to_path_buf(),
        plugin_name: plugin_name.to_owned(),
        jar_file_name: jar_file_name.to_owned(),
    })
}

/// Follows symbolic links.
fn check_regular_file(path: &Path) -> Result<(), Error> {
    let metadata = fs::metadata(path).map_err(|source| Error::ExamineLocalJar {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err(Error::NotRegularFile {
            path: path.to_path_buf(),
        })
    }
}

impl UrlSource {
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The last element of the URL's path without `.jar`, with its percent-encoding kept: the
    /// part after `://` up to the first `?` or `#`, after its last `/` (fetch-2).
    pub fn plugin_name(&self) -> String {
        let (_, after_scheme) = self
            .url
            .split_once("://")
            .expect("a URL source starts with https:// or http://");
        let up_to_query = after_scheme
            .split(['?', '#'])
            .next()
            .expect("splitting yields at least one element");
        let last_element = up_to_query
            .rsplit_once('/')
            .map_or("", |(_, last_element)| last_element);
        last_element
            .strip_suffix(JAR_SUFFIX)
            .unwrap_or(last_element)
            .to_owned()
    }
}

/// `modrinth:`, the identifier, and each value after a `:` (plg-12).
pub fn modrinth_source(identifier: &str, values: &[&str]) -> String {
    let mut source = format!("{MODRINTH_PREFIX}{identifier}");
    for value in values {
        source.push(':');
        source.push_str(value);
    }
    source
}

/// The identifier in a recorded source: the first element after `modrinth:`. Other sources have
/// none.
pub fn identifier_of(source: &str) -> Option<&str> {
    source.strip_prefix(MODRINTH_PREFIX)?.split(':').next()
}

/// The version number in a recorded source: the second element after `modrinth:`. Other sources
/// and a `modrinth:` source without it have none.
pub fn version_number_of(source: &str) -> Option<&str> {
    source.strip_prefix(MODRINTH_PREFIX)?.split(':').nth(1)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn url_source(url: &str) -> UrlSource {
        match parse(url).unwrap() {
            PluginSource::Url(source) => source,
            other => panic!("{url} parsed as {other:?}"),
        }
    }

    fn path_text(path: &Path) -> &str {
        path.to_str().unwrap()
    }

    /// plg-3: `modrinth:` gives the identifier and the values in order, empty ones included.
    #[test]
    fn modrinth_source_gives_the_identifier_and_the_values() {
        assert_eq!(
            parse("modrinth:fastasyncworldedit:2.16.1::paper").unwrap(),
            PluginSource::Modrinth(ModrinthSource {
                identifier: "fastasyncworldedit".to_owned(),
                values: vec!["2.16.1".to_owned(), String::new(), "paper".to_owned()],
            })
        );
    }

    /// plg-3: `modrinth:` and an identifier only has no values.
    #[test]
    fn modrinth_source_without_values_has_none() {
        assert_eq!(
            parse("modrinth:luckperms").unwrap(),
            PluginSource::Modrinth(ModrinthSource {
                identifier: "luckperms".to_owned(),
                values: Vec::new(),
            })
        );
    }

    /// plg-3: `https://` and `http://` begin a URL.
    #[test]
    fn https_and_http_begin_a_url() {
        let urls: Vec<_> = ["https://example.com/a.jar", "http://example.com/a.jar"]
            .into_iter()
            .filter(|text| matches!(parse(text), Ok(PluginSource::Url(_))))
            .collect();
        assert_eq!(urls.len(), 2);
    }

    /// 「取得元の指定」 方法: the URL prefixes are compared with case, so `HTTPS://` is a path.
    #[test]
    fn upper_case_scheme_is_not_a_url() {
        assert!(matches!(
            parse("HTTPS://example.com/a.jar"),
            Err(Error::ExamineLocalJar { .. })
        ));
    }

    /// plg-3: a string that is none of the three forms is an error with the forms as the hint.
    #[test]
    fn missing_local_file_is_an_error_with_the_three_forms_as_hint() {
        let directory = temporary_directory();
        let missing = directory.path().join("missing.jar");

        let error = parse(path_text(&missing)).unwrap_err();

        assert_eq!(error.hint().as_deref(), Some(THREE_FORMS));
    }

    /// d-source-3
    #[test]
    fn identifier_with_a_slash_is_rejected() {
        assert!(matches!(
            parse("modrinth:../search"),
            Err(Error::InvalidIdentifier { .. })
        ));
    }

    /// plg-3: identifiers have 3 to 64 characters.
    #[test]
    fn identifiers_of_3_and_64_characters_are_accepted() {
        let rejected: Vec<_> = ["abc".to_owned(), "a".repeat(64)]
            .into_iter()
            .filter(|identifier| check_identifier(identifier).is_err())
            .collect();
        assert!(rejected.is_empty(), "{rejected:?}");
    }

    /// plg-3: identifiers of 2 and 65 characters are rejected.
    #[test]
    fn identifiers_of_2_and_65_characters_are_rejected() {
        let accepted: Vec<_> = ["ab".to_owned(), "a".repeat(65)]
            .into_iter()
            .filter(|identifier| check_identifier(identifier).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// plg-3: every character of the documented set is accepted.
    #[test]
    fn documented_punctuation_is_accepted() {
        assert!(check_identifier("Az09_!@$()`.+,\"-'").is_ok());
    }

    /// plg-3: characters outside the set (`/`, `:`, space, non-ASCII) are rejected.
    #[test]
    fn other_characters_are_rejected() {
        let accepted: Vec<_> = ["a/bc", "a bc", "abcé", "ab#c", "ab%c"]
            .into_iter()
            .filter(|identifier| check_identifier(identifier).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// plg-3, plg-5: a local jar file gives its plugin name and jar file name.
    #[test]
    fn local_jar_gives_its_names() {
        let directory = temporary_directory();
        let path = directory.path().join("Example-1.0.jar");
        fs::write(&path, b"jar").unwrap();

        assert_eq!(
            parse(path_text(&path)).unwrap(),
            PluginSource::File(LocalJar {
                path: path.clone(),
                plugin_name: "Example-1.0".to_owned(),
                jar_file_name: "Example-1.0.jar".to_owned(),
            })
        );
    }

    /// 「取得元の指定」 方法: a symbolic link to a regular file is a local jar file.
    #[test]
    fn symbolic_link_to_a_jar_is_a_local_jar() {
        let directory = temporary_directory();
        let target = directory.path().join("target.bin");
        fs::write(&target, b"jar").unwrap();
        let link = directory.path().join("Example.jar");
        symlink(&target, &link).unwrap();

        assert!(matches!(parse(path_text(&link)), Ok(PluginSource::File(_))));
    }

    /// 「取得元の指定」 方法: a directory is not a local jar file.
    #[test]
    fn directory_is_not_a_local_jar() {
        let directory = temporary_directory();
        let path = directory.path().join("Example.jar");
        fs::create_dir(&path).unwrap();

        assert!(matches!(
            parse(path_text(&path)),
            Err(Error::NotRegularFile { .. })
        ));
    }

    /// d-source-1
    #[test]
    fn local_file_whose_name_does_not_end_with_jar_is_rejected() {
        let directory = temporary_directory();
        let path = directory.path().join("Example.zip");
        fs::write(&path, b"jar").unwrap();

        assert!(matches!(
            parse(path_text(&path)),
            Err(Error::NotJarName { .. })
        ));
    }

    /// plg-6: a local file named `.jar` has an empty plugin name.
    #[test]
    fn local_file_named_dot_jar_is_rejected() {
        let directory = temporary_directory();
        let path = directory.path().join(".jar");
        fs::write(&path, b"jar").unwrap();

        assert!(matches!(parse(path_text(&path)), Err(Error::FileName(_))));
    }

    /// vers-4: `modrinth:` and an identifier is the argument of `mism plugin versions`.
    #[test]
    fn versions_argument_gives_the_identifier() {
        assert_eq!(
            parse_versions_argument("modrinth:luckperms").unwrap(),
            "luckperms"
        );
    }

    /// vers-4: without `modrinth:`, with a value after the identifier, or with an invalid
    /// identifier, the argument is rejected.
    #[test]
    fn other_versions_arguments_are_rejected() {
        let accepted: Vec<_> = [
            "luckperms",
            "modrinth:luckperms:v5.5.71-bukkit",
            "modrinth:luckperms:",
            "modrinth:lp",
            "https://example.com/a.jar",
        ]
        .into_iter()
        .filter(|text| parse_versions_argument(text).is_ok())
        .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// d-source-2
    #[test]
    fn plugin_name_from_a_url_keeps_percent_encoding_and_drops_query_and_fragment() {
        assert_eq!(
            url_source("https://example.com/dl/My%20Plugin.jar?token=1#top").plugin_name(),
            "My%20Plugin"
        );
    }

    /// 「取得元の指定」 方法: a URL without a `/` after its host has an empty last element.
    #[test]
    fn plugin_name_from_a_url_without_a_path_is_empty() {
        assert_eq!(url_source("https://example.com").plugin_name(), "");
    }

    /// fetch-2: a last element without `.jar` is the name as it is.
    #[test]
    fn plugin_name_from_a_url_without_jar_is_the_last_element() {
        assert_eq!(
            url_source("http://example.com/download#jar").plugin_name(),
            "download"
        );
    }

    /// plg-12
    #[test]
    fn modrinth_source_joins_the_values_with_colons() {
        assert_eq!(
            modrinth_source("fastasyncworldedit", &["2.16.1", "paper"]),
            "modrinth:fastasyncworldedit:2.16.1:paper"
        );
    }

    /// 「取得元の指定」 方法: the identifier of a recorded Modrinth source.
    #[test]
    fn identifier_of_a_modrinth_source_is_its_first_element() {
        assert_eq!(
            identifier_of("modrinth:luckperms:v5.5.71-bukkit"),
            Some("luckperms")
        );
    }

    /// 「取得元の指定」 方法: the version number of a recorded Modrinth source.
    #[test]
    fn version_number_of_a_modrinth_source_is_its_second_element() {
        assert_eq!(
            version_number_of("modrinth:fastasyncworldedit:2.16.1:paper"),
            Some("2.16.1")
        );
    }

    /// 「取得元の指定」 方法: URL and local jar sources have no identifier or version number.
    #[test]
    fn other_sources_have_no_version_number() {
        let found: Vec<_> = [
            "https://example.com/a.jar",
            "plugins/a.jar",
            "modrinth:luckperms",
        ]
        .into_iter()
        .filter_map(version_number_of)
        .collect();
        assert!(found.is_empty(), "{found:?}");
    }
}
