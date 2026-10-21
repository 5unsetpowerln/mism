use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::file_name;
use crate::modrinth_source::{self, ModrinthSource};

const URL_PREFIXES: [&str; 2] = ["https://", "http://"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Modrinth(#[from] modrinth_source::Error),
    #[error("{} is not a source: failed to examine it as a local file", path.display())]
    ExamineLocalFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a source: it is not a regular file", path.display())]
    NotRegularFile { path: PathBuf },
    #[error("{} is not a source: its name does not end with `{suffix}`", path.display())]
    WrongSuffix { path: PathBuf, suffix: &'static str },
    #[error(transparent)]
    FileName(#[from] file_name::Error),
}

impl Error {
    /// None for a string of none of the three forms: its remedy names the forms of the caller's
    /// files.
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Modrinth(error) => error.hint(),
            Error::FileName(error) => error.hint(),
            Error::ExamineLocalFile { .. }
            | Error::NotRegularFile { .. }
            | Error::WrongSuffix { .. } => None,
        }
    }
}

/// A <取得元の指定> (plg-3, dp-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchSource {
    Modrinth(ModrinthSource),
    Url(UrlSource),
    File(LocalFile),
}

impl FetchSource {
    /// The local file of a source of that form, whose content finds it in the cache directory
    /// (place-1, dplace-1).
    pub fn local_path(&self) -> Option<&Path> {
        match self {
            FetchSource::File(local_file) => Some(&local_file.path),
            FetchSource::Modrinth(_) | FetchSource::Url(_) => None,
        }
    }
}

/// A URL starting with `https://` or `http://`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlSource {
    url: String,
}

/// A local file that exists, with the names plg-5 and dp-8 give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFile {
    /// As given; a relative path is relative to the current directory.
    pub path: PathBuf,
    /// The file name without its suffix.
    pub name: String,
    pub file_name: String,
}

/// Parses a <取得元の指定> by its beginning; the name of a local file must end with `suffix`
/// (plg-3, dp-4).
pub fn parse(text: &str, suffix: &'static str) -> Result<FetchSource, Error> {
    if let Some(modrinth) = modrinth_source::parse(text)? {
        return Ok(FetchSource::Modrinth(modrinth));
    }
    if URL_PREFIXES.iter().any(|prefix| text.starts_with(prefix)) {
        return Ok(FetchSource::Url(UrlSource {
            url: text.to_owned(),
        }));
    }
    parse_local_file(Path::new(text), suffix).map(FetchSource::File)
}

fn parse_local_file(path: &Path, suffix: &'static str) -> Result<LocalFile, Error> {
    check_regular_file(path)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.ends_with(suffix))
        .ok_or_else(|| Error::WrongSuffix {
            path: path.to_path_buf(),
            suffix,
        })?;
    let name = file_name
        .strip_suffix(suffix)
        .expect("the name ends with the suffix");
    file_name::check_name(name)?;
    file_name::check_file_name(file_name)?;
    Ok(LocalFile {
        path: path.to_path_buf(),
        name: name.to_owned(),
        file_name: file_name.to_owned(),
    })
}

/// Follows symbolic links.
fn check_regular_file(path: &Path) -> Result<(), Error> {
    let metadata = fs::metadata(path).map_err(|source| Error::ExamineLocalFile {
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

    /// The last element of the URL's path without `suffix`, with its percent-encoding kept: the
    /// part after `://` up to the first `?` or `#`, after its last `/` (fetch-2, dfetch-2).
    pub fn name(&self, suffix: &str) -> String {
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
            .strip_suffix(suffix)
            .unwrap_or(last_element)
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    const JAR: &str = ".jar";
    const ZIP: &str = ".zip";

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn url_source(url: &str) -> UrlSource {
        match parse(url, JAR).unwrap() {
            FetchSource::Url(source) => source,
            other => panic!("{url} parsed as {other:?}"),
        }
    }

    fn path_text(path: &Path) -> &str {
        path.to_str().unwrap()
    }

    /// plg-3: `modrinth:` begins a Modrinth source.
    #[test]
    fn modrinth_prefix_begins_a_modrinth_source() {
        assert_eq!(
            parse("modrinth:luckperms:v5.5.71-bukkit", JAR).unwrap(),
            FetchSource::Modrinth(ModrinthSource {
                identifier: "luckperms".to_owned(),
                values: vec!["v5.5.71-bukkit".to_owned()],
            })
        );
    }

    /// plg-3: `https://` and `http://` begin a URL.
    #[test]
    fn https_and_http_begin_a_url() {
        let urls: Vec<_> = ["https://example.com/a.jar", "http://example.com/a.jar"]
            .into_iter()
            .filter(|text| matches!(parse(text, JAR), Ok(FetchSource::Url(_))))
            .collect();
        assert_eq!(urls.len(), 2);
    }

    /// 「取得元の指定」 方法: the URL prefixes are compared with case, so `HTTPS://` is a path.
    #[test]
    fn upper_case_scheme_is_not_a_url() {
        assert!(matches!(
            parse("HTTPS://example.com/a.jar", JAR),
            Err(Error::ExamineLocalFile { .. })
        ));
    }

    /// plg-3, plg-5: a local jar file gives its plugin name and jar file name.
    #[test]
    fn local_jar_gives_its_names() {
        let directory = temporary_directory();
        let path = directory.path().join("Example-1.0.jar");
        fs::write(&path, b"jar").unwrap();

        assert_eq!(
            parse(path_text(&path), JAR).unwrap(),
            FetchSource::File(LocalFile {
                path: path.clone(),
                name: "Example-1.0".to_owned(),
                file_name: "Example-1.0.jar".to_owned(),
            })
        );
    }

    /// dp-4, dp-8: a local datapack file gives its datapack name and file name.
    #[test]
    fn local_datapack_file_gives_its_names() {
        let directory = temporary_directory();
        let path = directory.path().join("Terralith_1.21_v2.5.zip");
        fs::write(&path, b"zip").unwrap();

        assert_eq!(
            parse(path_text(&path), ZIP).unwrap(),
            FetchSource::File(LocalFile {
                path: path.clone(),
                name: "Terralith_1.21_v2.5".to_owned(),
                file_name: "Terralith_1.21_v2.5.zip".to_owned(),
            })
        );
    }

    /// 「取得元の指定」 方法: a symbolic link to a regular file is a local file.
    #[test]
    fn symbolic_link_to_a_jar_is_a_local_jar() {
        let directory = temporary_directory();
        let target = directory.path().join("target.bin");
        fs::write(&target, b"jar").unwrap();
        let link = directory.path().join("Example.jar");
        symlink(&target, &link).unwrap();

        assert!(matches!(
            parse(path_text(&link), JAR),
            Ok(FetchSource::File(_))
        ));
    }

    /// 「取得元の指定」 方法: a directory is not a local file.
    #[test]
    fn directory_is_not_a_local_jar() {
        let directory = temporary_directory();
        let path = directory.path().join("Example.jar");
        fs::create_dir(&path).unwrap();

        assert!(matches!(
            parse(path_text(&path), JAR),
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
            parse(path_text(&path), JAR),
            Err(Error::WrongSuffix { .. })
        ));
    }

    /// d-source-4
    #[test]
    fn local_file_whose_name_does_not_end_with_zip_is_rejected_for_a_datapack() {
        let directory = temporary_directory();
        let path = directory.path().join("pack.jar");
        fs::write(&path, b"zip").unwrap();

        assert!(matches!(
            parse(path_text(&path), ZIP),
            Err(Error::WrongSuffix { .. })
        ));
    }

    /// plg-6: a local file named `.jar` has an empty plugin name.
    #[test]
    fn local_file_named_dot_jar_is_rejected() {
        let directory = temporary_directory();
        let path = directory.path().join(".jar");
        fs::write(&path, b"jar").unwrap();

        assert!(matches!(
            parse(path_text(&path), JAR),
            Err(Error::FileName(_))
        ));
    }

    /// d-source-2
    #[test]
    fn plugin_name_from_a_url_keeps_percent_encoding_and_drops_query_and_fragment() {
        assert_eq!(
            url_source("https://example.com/dl/My%20Plugin.jar?token=1#top").name(JAR),
            "My%20Plugin"
        );
    }

    /// dfetch-2: a datapack's name from a URL drops `.zip`, and keeps `.jar`.
    #[test]
    fn datapack_name_from_a_url_drops_zip_only() {
        assert_eq!(
            (
                url_source("https://example.com/dl/Pack.zip").name(ZIP),
                url_source("https://example.com/dl/Pack.jar").name(ZIP)
            ),
            ("Pack".to_owned(), "Pack.jar".to_owned())
        );
    }

    /// 「取得元の指定」 方法: a URL without a `/` after its host has an empty last element.
    #[test]
    fn plugin_name_from_a_url_without_a_path_is_empty() {
        assert_eq!(url_source("https://example.com").name(JAR), "");
    }

    /// fetch-2: a last element without `.jar` is the name as it is.
    #[test]
    fn plugin_name_from_a_url_without_jar_is_the_last_element() {
        assert_eq!(
            url_source("http://example.com/download#jar").name(JAR),
            "download"
        );
    }
}
