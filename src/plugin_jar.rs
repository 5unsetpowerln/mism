use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

use serde::Deserialize;
use zip::ZipArchive;
use zip::result::ZipError;

const PLUGIN_YML: &str = "plugin.yml";
const PAPER_PLUGIN_YML: &str = "paper-plugin.yml";
const BYTE_ORDER_MARK: char = '\u{feff}';

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {}", path.display())]
    ReadJar {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a zip file, so it is not a jar file", path.display())]
    NotZip {
        path: PathBuf,
        #[source]
        source: ZipError,
    },
    #[error("failed to read {entry} in {}", path.display())]
    ReadEntry {
        path: PathBuf,
        entry: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("{entry} in {} is not UTF-8", path.display())]
    NotUtf8 {
        path: PathBuf,
        entry: &'static str,
        #[source]
        source: FromUtf8Error,
    },
    #[error("{entry} in {} is not a valid plugin declaration", path.display())]
    InvalidDeclaration {
        path: PathBuf,
        entry: &'static str,
        // Boxed: the parser's error is large, and every result of this module carries its size.
        #[source]
        source: Box<serde_saphyr::Error>,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::ReadJar { .. }
            | Error::NotZip { .. }
            | Error::ReadEntry { .. }
            | Error::NotUtf8 { .. }
            | Error::InvalidDeclaration { .. } => None,
        }
    }
}

/// What a jar file's declaration (insp-1) says about its plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declaration {
    pub name: Option<String>,
    pub version: Option<String>,
    /// `api-version`, the minimum Minecraft version, as written.
    pub api_version: Option<String>,
    /// `folia-supported`; false without the key.
    pub folia_supported: bool,
    pub required_dependencies: BTreeSet<String>,
    pub optional_dependencies: BTreeSet<String>,
    /// The jar has `paper-plugin.yml` and no `plugin.yml` (insp-2 `paper`).
    pub paper_plugin_format: bool,
}

/// The declaration of the jar file at `path`: its top-level `plugin.yml`, or without it its
/// top-level `paper-plugin.yml`. `None` when the jar has neither (insp-4).
pub fn read_declaration(path: &Path) -> Result<Option<Declaration>, Error> {
    let file = File::open(path).map_err(|source| Error::ReadJar {
        path: path.to_path_buf(),
        source,
    })?;
    read_declaration_from(file, path)
}

fn read_declaration_from(jar: impl Read + Seek, path: &Path) -> Result<Option<Declaration>, Error> {
    let mut archive = ZipArchive::new(jar).map_err(|source| match source {
        ZipError::Io(source) => Error::ReadJar {
            path: path.to_path_buf(),
            source,
        },
        source => Error::NotZip {
            path: path.to_path_buf(),
            source,
        },
    })?;
    if let Some(text) = entry_text(&mut archive, PLUGIN_YML, path)? {
        return parse_declaration::<PluginYml>(&text, PLUGIN_YML, path).map(Some);
    }
    match entry_text(&mut archive, PAPER_PLUGIN_YML, path)? {
        Some(text) => parse_declaration::<PaperPluginYml>(&text, PAPER_PLUGIN_YML, path).map(Some),
        None => Ok(None),
    }
}

/// The text of the entry whose path is exactly `entry`, without a leading byte order mark.
fn entry_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry: &'static str,
    path: &Path,
) -> Result<Option<String>, Error> {
    let Some(index) = archive.index_for_name(entry) else {
        return Ok(None);
    };
    let read_failed = |source| Error::ReadEntry {
        path: path.to_path_buf(),
        entry,
        source,
    };
    let mut bytes = Vec::new();
    archive
        .by_index(index)
        .map_err(|source| read_failed(source.into()))?
        .read_to_end(&mut bytes)
        .map_err(read_failed)?;
    let text = String::from_utf8(bytes).map_err(|source| Error::NotUtf8 {
        path: path.to_path_buf(),
        entry,
        source,
    })?;
    Ok(Some(match text.strip_prefix(BYTE_ORDER_MARK) {
        Some(rest) => rest.to_owned(),
        None => text,
    }))
}

fn parse_declaration<'a, T: Deserialize<'a> + Into<Declaration>>(
    text: &'a str,
    entry: &'static str,
    path: &Path,
) -> Result<Declaration, Error> {
    serde_saphyr::from_str::<T>(text)
        .map(Into::into)
        .map_err(|source| Error::InvalidDeclaration {
            path: path.to_path_buf(),
            entry,
            source: Box::new(source),
        })
}

/// The keys of `plugin.yml` that mism reads. The values mism shows are strings as written.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PluginYml {
    name: Option<String>,
    version: Option<String>,
    api_version: Option<String>,
    folia_supported: Option<bool>,
    depend: Option<Vec<String>>,
    softdepend: Option<Vec<String>>,
}

impl From<PluginYml> for Declaration {
    fn from(yml: PluginYml) -> Declaration {
        Declaration {
            name: yml.name,
            version: yml.version,
            api_version: yml.api_version,
            folia_supported: yml.folia_supported.unwrap_or(false),
            required_dependencies: yml.depend.into_iter().flatten().collect(),
            optional_dependencies: yml.softdepend.into_iter().flatten().collect(),
            paper_plugin_format: false,
        }
    }
}

/// The keys of `paper-plugin.yml` that mism reads.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PaperPluginYml {
    name: Option<String>,
    version: Option<String>,
    api_version: Option<String>,
    folia_supported: Option<bool>,
    dependencies: Option<PaperDependencies>,
}

/// Each section maps the name of a dependency to its settings, which may be empty.
#[derive(Deserialize)]
struct PaperDependencies {
    bootstrap: Option<BTreeMap<String, Option<PaperDependency>>>,
    server: Option<BTreeMap<String, Option<PaperDependency>>>,
}

#[derive(Deserialize)]
struct PaperDependency {
    required: Option<bool>,
}

impl PaperDependency {
    /// `required` defaults to true.
    fn is_required(settings: Option<&PaperDependency>) -> bool {
        settings
            .and_then(|settings| settings.required)
            .unwrap_or(true)
    }
}

impl From<PaperPluginYml> for Declaration {
    fn from(yml: PaperPluginYml) -> Declaration {
        let (required_dependencies, optional_dependencies) = yml
            .dependencies
            .map(split_paper_dependencies)
            .unwrap_or_default();
        Declaration {
            name: yml.name,
            version: yml.version,
            api_version: yml.api_version,
            folia_supported: yml.folia_supported.unwrap_or(false),
            required_dependencies,
            optional_dependencies,
            paper_plugin_format: true,
        }
    }
}

/// The required and the optional dependencies of both sections. A name required in either
/// section is only required.
fn split_paper_dependencies(
    dependencies: PaperDependencies,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut required = BTreeSet::new();
    let mut optional = BTreeSet::new();
    for (name, settings) in [dependencies.bootstrap, dependencies.server]
        .into_iter()
        .flatten()
        .flatten()
    {
        if PaperDependency::is_required(settings.as_ref()) {
            required.insert(name);
        } else {
            optional.insert(name);
        }
    }
    optional.retain(|name| !required.contains(name));
    (required, optional)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::io::{Cursor, Write};
    use std::path::Path;

    use zip::write::SimpleFileOptions;

    use super::{Declaration, Error, read_declaration_from};

    /// A zip archive with each entry's path and content.
    fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn declaration_of(entries: &[(&str, &[u8])]) -> Result<Option<Declaration>, Error> {
        read_declaration_from(Cursor::new(jar(entries)), Path::new("plugin.jar"))
    }

    fn declaration(entries: &[(&str, &[u8])]) -> Declaration {
        declaration_of(entries).unwrap().unwrap()
    }

    fn names(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    /// d-jar-1
    #[test]
    fn unquoted_numbers_are_read_as_written() {
        let read = declaration(&[(
            "plugin.yml",
            b"name: Example\nversion: 2.0\napi-version: 1.20\nmain: a.B\n",
        )]);

        assert_eq!(
            (read.api_version.as_deref(), read.version.as_deref()),
            (Some("1.20"), Some("2.0"))
        );
    }

    /// d-jar-2
    #[test]
    fn plugin_yml_is_read_when_both_declarations_exist() {
        let read = declaration(&[
            ("paper-plugin.yml", b"name: FromPaperPluginYml\n"),
            ("plugin.yml", b"name: FromPluginYml\n"),
        ]);

        assert_eq!(read.name.as_deref(), Some("FromPluginYml"));
    }

    /// d-jar-3
    #[test]
    fn paper_dependency_without_required_is_required() {
        let read = declaration(&[(
            "paper-plugin.yml",
            b"name: Example\ndependencies:\n  bootstrap:\n    A:\n      load: BEFORE\n  server:\n    B:\n      required: false\n",
        )]);

        assert_eq!(
            (read.required_dependencies, read.optional_dependencies),
            (names(&["A"]), names(&["B"]))
        );
    }

    /// d-jar-4
    #[test]
    fn paper_dependency_required_in_one_section_is_only_required() {
        let read = declaration(&[(
            "paper-plugin.yml",
            b"name: Example\ndependencies:\n  bootstrap:\n    A:\n      required: false\n  server:\n    A:\n      required: true\n",
        )]);

        assert_eq!(
            (read.required_dependencies, read.optional_dependencies),
            (names(&["A"]), names(&[]))
        );
    }

    /// d-jar-5
    #[test]
    fn depend_that_is_not_a_list_is_an_error() {
        assert!(matches!(
            declaration_of(&[("plugin.yml", b"name: Example\ndepend: Vault\n")]),
            Err(Error::InvalidDeclaration { .. })
        ));
    }

    /// d-jar-6
    #[test]
    fn plugin_yml_below_a_directory_is_no_declaration() {
        assert_eq!(
            declaration_of(&[("META-INF/plugin.yml", b"name: Example\n")]).unwrap(),
            None
        );
    }

    /// d-jar-7
    #[test]
    fn file_that_is_not_zip_is_a_distinct_error() {
        let result = read_declaration_from(
            Cursor::new(b"not a zip file".to_vec()),
            Path::new("plugin.jar"),
        );

        assert!(matches!(result, Err(Error::NotZip { .. })));
    }

    /// insp-4: a jar with neither declaration has none.
    #[test]
    fn jar_without_declarations_has_none() {
        assert_eq!(
            declaration_of(&[("a/B.class", b"\xca\xfe\xba\xbe")]).unwrap(),
            None
        );
    }

    /// 「jar ファイルの宣言の読み取り」 方法: `plugin.yml` gives every key mism reads.
    #[test]
    fn plugin_yml_gives_every_key() {
        let read = declaration(&[(
            "plugin.yml",
            b"name: Example\nversion: '1.0'\napi-version: '1.21'\nfolia-supported: true\ndepend: [Vault, ProtocolLib]\nsoftdepend:\n  - PlaceholderAPI\n",
        )]);

        assert_eq!(
            read,
            Declaration {
                name: Some("Example".to_owned()),
                version: Some("1.0".to_owned()),
                api_version: Some("1.21".to_owned()),
                folia_supported: true,
                required_dependencies: names(&["ProtocolLib", "Vault"]),
                optional_dependencies: names(&["PlaceholderAPI"]),
                paper_plugin_format: false,
            }
        );
    }

    /// 「jar ファイルの宣言の読み取り」 方法: missing keys and empty dependency lists are absent.
    #[test]
    fn missing_keys_and_null_lists_are_absent() {
        assert_eq!(
            declaration(&[("plugin.yml", b"main: a.B\ndepend:\nsoftdepend: ~\n")]),
            Declaration::default()
        );
    }

    /// insp-2: `paper-plugin.yml` without `plugin.yml` is the Paper plugin format.
    #[test]
    fn paper_plugin_yml_alone_is_the_paper_plugin_format() {
        assert!(declaration(&[("paper-plugin.yml", b"name: Example\n")]).paper_plugin_format);
    }

    /// 「jar ファイルの宣言の読み取り」 方法: a leading byte order mark is removed.
    #[test]
    fn leading_byte_order_mark_is_removed() {
        let read = declaration(&[("plugin.yml", "\u{feff}name: Example\r\n".as_bytes())]);

        assert_eq!(read.name.as_deref(), Some("Example"));
    }

    /// 「jar ファイルの宣言の読み取り」 方法: a mapping with the same key twice is an error.
    #[test]
    fn duplicate_key_is_an_error() {
        assert!(matches!(
            declaration_of(&[("plugin.yml", b"name: A\nname: B\n")]),
            Err(Error::InvalidDeclaration { .. })
        ));
    }

    /// 「jar ファイルの宣言の読み取り」 方法: a declaration that is not UTF-8 is an error.
    #[test]
    fn declaration_that_is_not_utf8_is_an_error() {
        assert!(matches!(
            declaration_of(&[("plugin.yml", b"name: \xff\n")]),
            Err(Error::NotUtf8 { .. })
        ));
    }
}
