use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_fs;

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
        // Boxed: the parser's error is large, and every result that carries this error carries
        // its size.
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("failed to write {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: atomic_fs::Error,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        None
    }

    /// Whether reading failed because there is no file.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// The contents of a server directory's `mism.toml`: which server software it runs, which
/// version of it, the plugins mism placed in its `plugins/`, and the datapacks mism placed in its
/// world's `datapacks/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MismToml {
    pub software: String,
    pub minecraft_version: String,
    pub software_version: u64,
    /// In the order mism placed them (plg-4). A file without `plugins` has no records.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Record>,
    /// In the order mism placed them (dp-7). A file without `datapacks` has no records.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datapacks: Vec<Record>,
}

/// The record of one plugin or datapack that mism placed (plg-4, dp-7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub name: String,
    pub source: String,
    pub file_name: String,
    pub sha256: String,
}

impl MismToml {
    pub fn read(path: &Path) -> Result<MismToml, Error> {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })
    }

    /// Writes the file by atomic replacement, so that it has either the old or the new content
    /// however mism ends (plg-8).
    pub fn write(&self, path: &Path) -> Result<(), Error> {
        let text = toml::to_string(self)
            .expect("strings, an unsigned integer, and an array of tables of strings are TOML");
        atomic_fs::replace(path, text.as_bytes()).map_err(|source| Error::Write {
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

    use super::Record;

    fn paper_26_3_206() -> MismToml {
        MismToml {
            software: "paper".to_owned(),
            minecraft_version: "26.3".to_owned(),
            software_version: 206,
            plugins: Vec::new(),
            datapacks: Vec::new(),
        }
    }

    fn luckperms_record() -> Record {
        Record {
            name: "LuckPerms".to_owned(),
            source: "modrinth:luckperms:v5.5.71-bukkit".to_owned(),
            file_name: "LuckPerms-Bukkit-5.5.71.jar".to_owned(),
            sha256: "0".repeat(64),
        }
    }

    fn read_text(content: &str) -> MismToml {
        let directory = temporary_directory();
        let path = directory.path().join("mism.toml");
        fs::write(&path, content).unwrap();
        MismToml::read(&path).unwrap()
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

    /// d-toml-1
    #[test]
    fn file_without_plugins_has_no_records() {
        let read = read_text(
            "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206\n",
        );

        assert_eq!(read.plugins, []);
    }

    /// d-toml-3
    #[test]
    fn file_without_datapacks_has_no_datapack_records() {
        let read = read_text(&format!(
            "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206\n\n\
             [[plugins]]\nname = \"LuckPerms\"\nsource = \"modrinth:luckperms:v5.5.71-bukkit\"\n\
             file_name = \"LuckPerms-Bukkit-5.5.71.jar\"\nsha256 = \"{}\"\n",
            "0".repeat(64)
        ));

        assert_eq!(read.datapacks, []);
    }

    /// 「mism.toml」 方法 (dp-7): datapack records are written as `[[datapacks]]` after the plugin
    /// records and read back in order.
    #[test]
    fn datapack_records_are_written_after_the_plugin_records() {
        let directory = temporary_directory();
        let path = directory.path().join("mism.toml");
        let true_ending = Record {
            name: "True Ending - Ender Dragon Overhaul".to_owned(),
            source: "modrinth:true-ending:1.1.4d".to_owned(),
            file_name: "tru.e-ending-1.1.4d.zip".to_owned(),
            sha256: "1".repeat(64),
        };
        MismToml {
            plugins: vec![luckperms_record()],
            datapacks: vec![true_ending],
            ..paper_26_3_206()
        }
        .write(&path)
        .unwrap();

        let text = fs::read_to_string(&path).unwrap();
        let tables: Vec<_> = text.lines().filter(|line| line.starts_with("[[")).collect();
        assert_eq!(tables, ["[[plugins]]", "[[datapacks]]"]);
    }

    /// d-toml-2
    #[test]
    fn file_without_records_has_only_the_three_keys() {
        let directory = temporary_directory();
        let path = directory.path().join("mism.toml");

        paper_26_3_206().write(&path).unwrap();

        let table: toml::Table = fs::read_to_string(&path).unwrap().parse().unwrap();
        let keys: Vec<_> = table.keys().map(String::as_str).collect();
        assert_eq!(keys, ["minecraft_version", "software", "software_version"]);
    }

    /// 「mism.toml」 方法 (plg-4): records are written as `[[plugins]]` and read back in order.
    #[test]
    fn records_are_read_back_in_order() {
        let directory = temporary_directory();
        let path = directory.path().join("mism.toml");
        let second = Record {
            name: "Vault".to_owned(),
            ..luckperms_record()
        };
        let written = MismToml {
            plugins: vec![luckperms_record(), second],
            ..paper_26_3_206()
        };
        written.write(&path).unwrap();

        assert_eq!(MismToml::read(&path).unwrap(), written);
    }

    /// 「mism.toml」 方法: the documented layout of a record.
    #[test]
    fn records_are_read_from_the_documented_layout() {
        let read = read_text(&format!(
            "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206\n\n\
             [[plugins]]\nname = \"LuckPerms\"\nsource = \"modrinth:luckperms:v5.5.71-bukkit\"\n\
             file_name = \"LuckPerms-Bukkit-5.5.71.jar\"\nsha256 = \"{}\"\n",
            "0".repeat(64)
        ));

        assert_eq!(read.plugins, [luckperms_record()]);
    }

    /// 「mism.toml」 方法: an unknown field in a record is an error naming the file and the field.
    #[test]
    fn unknown_field_in_a_record_names_the_file_and_the_field() {
        let (path, report) = read_failure(
            "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206\n\n\
             [[plugins]]\nname = \"A\"\nsource = \"a.jar\"\nfile_name = \"a.jar\"\nsha256 = \"0\"\n\
             version = \"1\"\n",
        );

        assert!(
            report.contains(&path) && report.contains("version"),
            "{report}"
        );
    }

    /// 「mism plugin と mism datapack の対象のサーバーと共通の手順」 方法 1: a missing file is told
    /// apart.
    #[test]
    fn reading_a_missing_file_is_a_not_found_error() {
        let directory = temporary_directory();

        let error = MismToml::read(&directory.path().join("mism.toml")).unwrap_err();

        assert!(error.is_not_found());
    }
}
