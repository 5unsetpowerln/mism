use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use zip::ZipArchive;
use zip::result::ZipError;

const VERSIONS_DIRECTORY_NAME: &str = "versions";
const VERSION_JSON: &str = "version.json";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the server jar {} does not exist", path.display())]
    Missing { path: PathBuf },
    #[error("failed to read the server jar {}", path.display())]
    ReadJar {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the server jar {} is not a zip file", path.display())]
    NotZip {
        path: PathBuf,
        #[source]
        source: ZipError,
    },
    #[error("the server jar {} has no {VERSION_JSON} at its top level", path.display())]
    NoVersionJson { path: PathBuf },
    #[error("failed to read {VERSION_JSON} in the server jar {}", path.display())]
    ReadVersionJson {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{VERSION_JSON} in the server jar {} is not valid", path.display())]
    InvalidVersionJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "the pack_version of {VERSION_JSON} in the server jar {} has neither data_major and \
         data_minor nor data",
        path.display()
    )]
    NoDataPackFormat { path: PathBuf },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Missing { path } => Some(format!(
                "start the server once so that it creates {}",
                path.display()
            )),
            Error::ReadJar { .. }
            | Error::NotZip { .. }
            | Error::NoVersionJson { .. }
            | Error::ReadVersionJson { .. }
            | Error::InvalidVersionJson { .. }
            | Error::NoDataPackFormat { .. } => None,
        }
    }
}

/// The data pack format number of a Minecraft version: its major and minor numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPackFormat {
    pub major: u32,
    pub minor: u32,
}

impl fmt::Display for DataPackFormat {
    /// `<major>.<minor>` (e.g. `121.0`).
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// The data pack format number of the server's Minecraft version, read from
/// `versions/<minecraft_version>/<software>-<minecraft_version>.jar` in `server_directory`
/// (dinsp-3, dinsp-5).
pub fn read_data_pack_format(
    server_directory: &Path,
    software: &str,
    minecraft_version: &str,
) -> Result<DataPackFormat, Error> {
    let path = server_directory
        .join(VERSIONS_DIRECTORY_NAME)
        .join(minecraft_version)
        .join(format!("{software}-{minecraft_version}.jar"));
    let file = File::open(&path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            Error::Missing { path: path.clone() }
        } else {
            Error::ReadJar {
                path: path.clone(),
                source,
            }
        }
    })?;
    read_from(file, &path)
}

fn read_from(jar: impl Read + Seek, path: &Path) -> Result<DataPackFormat, Error> {
    let version_json: VersionJson = serde_json::from_slice(&version_json_bytes(jar, path)?)
        .map_err(|source| Error::InvalidVersionJson {
            path: path.to_path_buf(),
            source,
        })?;
    data_pack_format(&version_json.pack_version).ok_or_else(|| Error::NoDataPackFormat {
        path: path.to_path_buf(),
    })
}

/// The content of the entry whose path is exactly `version.json`.
fn version_json_bytes(jar: impl Read + Seek, path: &Path) -> Result<Vec<u8>, Error> {
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
    let index = archive
        .index_for_name(VERSION_JSON)
        .ok_or_else(|| Error::NoVersionJson {
            path: path.to_path_buf(),
        })?;
    let read_failed = |source| Error::ReadVersionJson {
        path: path.to_path_buf(),
        source,
    };
    let mut bytes = Vec::new();
    archive
        .by_index(index)
        .map_err(|source| read_failed(source.into()))?
        .read_to_end(&mut bytes)
        .map_err(read_failed)?;
    Ok(bytes)
}

/// Steps 1 and 2: `data_major` and `data_minor`, else `data` with the minor number 0.
fn data_pack_format(pack_version: &PackVersion) -> Option<DataPackFormat> {
    match (
        pack_version.data_major,
        pack_version.data_minor,
        pack_version.data,
    ) {
        (Some(major), Some(minor), _) => Some(DataPackFormat { major, minor }),
        (_, _, Some(major)) => Some(DataPackFormat { major, minor: 0 }),
        _ => None,
    }
}

/// The key of `version.json` that mism reads.
#[derive(Deserialize)]
struct VersionJson {
    pack_version: PackVersion,
}

#[derive(Deserialize)]
struct PackVersion {
    data_major: Option<u32>,
    data_minor: Option<u32>,
    data: Option<u32>,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Cursor, Write};
    use std::path::Path;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

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

    /// A jar whose top-level `version.json` has `pack_version`.
    fn jar_with_pack_version(pack_version: &str) -> Vec<u8> {
        jar(&[(
            "version.json",
            format!(r#"{{"id": "26.3", "name": "26.3", "pack_version": {pack_version}}}"#)
                .as_bytes(),
        )])
    }

    fn format_of(jar: Vec<u8>) -> Result<DataPackFormat, Error> {
        read_from(Cursor::new(jar), Path::new("versions/26.3/paper-26.3.jar"))
    }

    /// d-sjar-1
    #[test]
    fn data_major_and_data_minor_are_the_format_number() {
        let format = format_of(jar_with_pack_version(
            r#"{"resource_major": 88, "resource_minor": 0, "data_major": 121, "data_minor": 0}"#,
        ))
        .unwrap();

        assert_eq!(format.to_string(), "121.0");
    }

    /// 「サーバーのデータパックの形式の番号」 方法 2: `data` is the major number with the minor
    /// number 0.
    #[test]
    fn data_is_the_major_number_with_minor_zero() {
        assert_eq!(
            format_of(jar_with_pack_version(r#"{"resource": 46, "data": 71}"#)).unwrap(),
            DataPackFormat {
                major: 71,
                minor: 0
            }
        );
    }

    /// 「サーバーのデータパックの形式の番号」 方法 1: `data_major` and `data_minor` come before
    /// `data`.
    #[test]
    fn data_major_and_data_minor_come_before_data() {
        assert_eq!(
            format_of(jar_with_pack_version(
                r#"{"data": 71, "data_major": 121, "data_minor": 1}"#
            ))
            .unwrap(),
            DataPackFormat {
                major: 121,
                minor: 1
            }
        );
    }

    /// 「サーバーのデータパックの形式の番号」 方法: a `pack_version` of neither form is an error.
    #[test]
    fn pack_version_of_neither_form_is_an_error() {
        assert!(matches!(
            format_of(jar_with_pack_version(
                r#"{"data_major": 121, "resource": 46}"#
            )),
            Err(Error::NoDataPackFormat { .. })
        ));
    }

    /// 「サーバーのデータパックの形式の番号」 方法: a `pack_version` that is one integer (before
    /// Minecraft 1.17) is not read.
    #[test]
    fn integer_pack_version_is_an_error() {
        assert!(matches!(
            format_of(jar_with_pack_version("6")),
            Err(Error::InvalidVersionJson { .. })
        ));
    }

    /// 「サーバーのデータパックの形式の番号」 方法: a jar without a top-level `version.json` is an
    /// error.
    #[test]
    fn jar_without_version_json_is_an_error() {
        assert!(matches!(
            format_of(jar(&[("META-INF/version.json", b"{}")])),
            Err(Error::NoVersionJson { .. })
        ));
    }

    /// 「サーバーのデータパックの形式の番号」 方法: a jar that is not a zip file is an error.
    #[test]
    fn jar_that_is_not_zip_is_an_error() {
        assert!(matches!(
            format_of(b"not a zip file".to_vec()),
            Err(Error::NotZip { .. })
        ));
    }

    /// dinsp-5: without the server jar the error hints at starting the server once.
    #[test]
    fn missing_server_jar_hints_at_starting_the_server() {
        let directory = temporary_directory();

        let error = read_data_pack_format(directory.path(), "paper", "26.3").unwrap_err();

        assert_eq!(
            error.hint(),
            Some(format!(
                "start the server once so that it creates {}",
                directory
                    .path()
                    .join("versions/26.3/paper-26.3.jar")
                    .display()
            ))
        );
    }

    /// 「サーバーのデータパックの形式の番号」 方法: the jar of the server software and the Minecraft
    /// version in `versions/` is read.
    #[test]
    fn jar_of_the_software_and_the_minecraft_version_is_read() {
        let directory = temporary_directory();
        let versions = directory.path().join("versions/26.3");
        fs::create_dir_all(&versions).unwrap();
        fs::write(
            versions.join("folia-26.3.jar"),
            jar_with_pack_version(r#"{"data_major": 121, "data_minor": 0}"#),
        )
        .unwrap();

        assert_eq!(
            read_data_pack_format(directory.path(), "folia", "26.3").unwrap(),
            DataPackFormat {
                major: 121,
                minor: 0
            }
        );
    }
}
