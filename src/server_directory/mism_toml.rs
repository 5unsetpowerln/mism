use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a valid mism.toml", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to write {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        None
    }
}

/// The contents of a server directory's `mism.toml`: which server software it runs, and which
/// version of it.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MismToml {
    pub software: String,
    pub minecraft_version: String,
    pub software_version: u64,
}

impl MismToml {
    // The design's reading of mism.toml serves the subcommands for existing server directories
    // (start, stop, update), none of which exists yet; `mism new` only writes it.
    #[allow(dead_code)]
    pub fn read(path: &Path) -> Result<MismToml, Error> {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn write(&self, path: &Path) -> Result<(), Error> {
        let text = toml::to_string(self)
            .expect("a table of two strings and an unsigned integer is representable in TOML");
        fs::write(path, text).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::MismToml;

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn paper_26_3_206() -> MismToml {
        MismToml {
            software: "paper".to_owned(),
            minecraft_version: "26.3".to_owned(),
            software_version: 206,
        }
    }

    /// Reads `content` as an `mism.toml` and returns the file's path and the reported error with
    /// its causes, as `main.rs` prints them.
    fn read_failure(content: &str) -> (String, String) {
        let directory = temporary_directory();
        let path = directory.path().join("mism.toml");
        fs::write(&path, content).unwrap();
        let error = MismToml::read(&path).unwrap_err();
        let mut report = error.to_string();
        let mut source = error.source();
        while let Some(cause) = source {
            report.push_str(&format!("\ncaused by: {cause}"));
            source = cause.source();
        }
        (path.display().to_string(), report)
    }

    /// 「mism.toml」 方法: the written file has exactly the documented three lines.
    #[test]
    fn write_produces_the_documented_layout() {
        let directory = temporary_directory();
        let path = directory.path().join("mism.toml");

        paper_26_3_206().write(&path).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206\n"
        );
    }

    /// 「mism.toml」 方法 (new-9): reading a written file gives back the three recorded values.
    #[test]
    fn read_returns_what_write_wrote() {
        let directory = temporary_directory();
        let path = directory.path().join("mism.toml");
        paper_26_3_206().write(&path).unwrap();

        let read = MismToml::read(&path).unwrap();

        assert_eq!(read, paper_26_3_206());
    }

    /// 「mism.toml」 方法: a missing field is an error naming the file and the field.
    #[test]
    fn missing_field_error_names_the_file_and_the_field() {
        let (path, report) = read_failure("software = \"paper\"\nminecraft_version = \"26.3\"\n");

        assert!(
            report.contains(&path) && report.contains("software_version"),
            "{report}"
        );
    }

    /// 「mism.toml」 方法: an unknown field is an error naming the file and the field.
    #[test]
    fn unknown_field_error_names_the_file_and_the_field() {
        let (path, report) = read_failure(
            "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206\nport = 25565\n",
        );

        assert!(
            report.contains(&path) && report.contains("port"),
            "{report}"
        );
    }

    /// 「mism.toml」 方法: a field of the wrong type is an error naming the file and the field.
    #[test]
    fn wrong_type_error_names_the_file_and_the_field() {
        let (path, report) = read_failure(
            "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = \"206\"\n",
        );

        assert!(
            report.contains(&path) && report.contains("software_version"),
            "{report}"
        );
    }
}
