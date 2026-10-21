use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

use serde::Deserialize;
use zip::ZipArchive;
use zip::result::ZipError;

const PACK_MCMETA: &str = "pack.mcmeta";
const BYTE_ORDER_MARK: char = '\u{feff}';

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {}", path.display())]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a zip file, so it is not a datapack file", path.display())]
    NotZip {
        path: PathBuf,
        #[source]
        source: ZipError,
    },
    #[error("{} has no pack.mcmeta at its top level, so it is not a datapack", path.display())]
    NoPackMcmeta { path: PathBuf },
    #[error("failed to read {PACK_MCMETA} in {}", path.display())]
    ReadPackMcmeta {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{PACK_MCMETA} in {} is not UTF-8", path.display())]
    NotUtf8 {
        path: PathBuf,
        #[source]
        source: FromUtf8Error,
    },
    #[error("{PACK_MCMETA} in {} is not a valid datapack declaration", path.display())]
    InvalidPackMcmeta {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{PACK_MCMETA} in {} has {present} but not {absent}", path.display())]
    OneFormatBound {
        path: PathBuf,
        present: &'static str,
        absent: &'static str,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::ReadFile { .. }
            | Error::NotZip { .. }
            | Error::NoPackMcmeta { .. }
            | Error::ReadPackMcmeta { .. }
            | Error::NotUtf8 { .. }
            | Error::InvalidPackMcmeta { .. }
            | Error::OneFormatBound { .. } => None,
        }
    }
}

/// A data pack format number as a declaration writes it: an integer, or two integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum FormatNumber {
    Major(u32),
    MajorMinor(u32, u32),
}

impl fmt::Display for FormatNumber {
    /// An integer as it is, two integers as `<first>.<second>` (dinsp-2).
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FormatNumber::Major(major) => write!(formatter, "{major}"),
            FormatNumber::MajorMinor(major, minor) => write!(formatter, "{major}.{minor}"),
        }
    }
}

/// The lowest and the highest data pack format number a declaration supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatRange {
    pub min: FormatNumber,
    pub max: FormatNumber,
}

/// The format numbers that the top-level `pack.mcmeta` of the datapack file at `path` declares,
/// `None` when it declares none (dinsp-2 `unknown`).
pub fn read_declaration(path: &Path) -> Result<Option<FormatRange>, Error> {
    let file = File::open(path).map_err(|source| Error::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    read_declaration_from(file, path)
}

fn read_declaration_from(
    datapack: impl Read + Seek,
    path: &Path,
) -> Result<Option<FormatRange>, Error> {
    let text = pack_mcmeta_text(datapack, path)?;
    let pack_mcmeta: PackMcmeta =
        serde_json::from_str(&text).map_err(|source| Error::InvalidPackMcmeta {
            path: path.to_path_buf(),
            source,
        })?;
    format_range(pack_mcmeta.pack, path)
}

/// The text of the entry whose path is exactly `pack.mcmeta`, without a leading byte order mark
/// (dinsp-4).
fn pack_mcmeta_text(datapack: impl Read + Seek, path: &Path) -> Result<String, Error> {
    let mut archive = ZipArchive::new(datapack).map_err(|source| match source {
        ZipError::Io(source) => Error::ReadFile {
            path: path.to_path_buf(),
            source,
        },
        source => Error::NotZip {
            path: path.to_path_buf(),
            source,
        },
    })?;
    let index = archive
        .index_for_name(PACK_MCMETA)
        .ok_or_else(|| Error::NoPackMcmeta {
            path: path.to_path_buf(),
        })?;
    let read_failed = |source| Error::ReadPackMcmeta {
        path: path.to_path_buf(),
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
        source,
    })?;
    Ok(match text.strip_prefix(BYTE_ORDER_MARK) {
        Some(rest) => rest.to_owned(),
        None => text,
    })
}

/// Steps 1 to 4 of 「データパックのファイルの宣言の読み取り」: `min_format` and `max_format`, else
/// `supported_formats`, else `pack_format`, else none.
fn format_range(pack: Pack, path: &Path) -> Result<Option<FormatRange>, Error> {
    let one_bound = |present, absent| Error::OneFormatBound {
        path: path.to_path_buf(),
        present,
        absent,
    };
    Ok(match (pack.min_format, pack.max_format) {
        (Some(min), Some(max)) => Some(FormatRange { min, max }),
        (Some(_), None) => return Err(one_bound("min_format", "max_format")),
        (None, Some(_)) => return Err(one_bound("max_format", "min_format")),
        (None, None) => pack
            .supported_formats
            .map(SupportedFormats::range)
            .or_else(|| pack.pack_format.map(|major| major_range(major, major))),
    })
}

fn major_range(min: u32, max: u32) -> FormatRange {
    FormatRange {
        min: FormatNumber::Major(min),
        max: FormatNumber::Major(max),
    }
}

/// The keys of `pack.mcmeta` that mism reads; it reads no other key.
#[derive(Deserialize)]
struct PackMcmeta {
    pack: Pack,
}

#[derive(Deserialize)]
struct Pack {
    min_format: Option<FormatNumber>,
    max_format: Option<FormatNumber>,
    supported_formats: Option<SupportedFormats>,
    pack_format: Option<u32>,
}

/// `supported_formats`: an integer, two integers, or a mapping of the two bounds.
#[derive(Deserialize)]
#[serde(untagged)]
enum SupportedFormats {
    Major(u32),
    Pair(u32, u32),
    Inclusive {
        min_inclusive: u32,
        max_inclusive: u32,
    },
}

impl SupportedFormats {
    fn range(self) -> FormatRange {
        match self {
            SupportedFormats::Major(major) => major_range(major, major),
            SupportedFormats::Pair(min, max) => major_range(min, max),
            SupportedFormats::Inclusive {
                min_inclusive,
                max_inclusive,
            } => major_range(min_inclusive, max_inclusive),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::path::Path;

    use zip::write::SimpleFileOptions;

    use super::{Error, FormatNumber, FormatRange, read_declaration_from};

    /// A zip archive with each entry's path and content.
    fn zip_file(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn declaration_of(entries: &[(&str, &[u8])]) -> Result<Option<FormatRange>, Error> {
        read_declaration_from(Cursor::new(zip_file(entries)), Path::new("pack.zip"))
    }

    fn declaration_of_pack(pack: &str) -> Result<Option<FormatRange>, Error> {
        declaration_of(&[("pack.mcmeta", format!(r#"{{"pack": {pack}}}"#).as_bytes())])
    }

    /// The lower and the upper bound as mism writes them.
    fn written_bounds(pack: &str) -> (String, String) {
        let range = declaration_of_pack(pack).unwrap().unwrap();
        (range.min.to_string(), range.max.to_string())
    }

    /// d-dpzip-1
    #[test]
    fn pack_mcmeta_below_a_directory_is_no_pack_mcmeta() {
        assert!(matches!(
            declaration_of(&[("inner/pack.mcmeta", br#"{"pack": {"pack_format": 71}}"#)]),
            Err(Error::NoPackMcmeta { .. })
        ));
    }

    /// d-dpzip-2
    #[test]
    fn file_that_is_not_zip_is_a_distinct_error() {
        let result = read_declaration_from(
            Cursor::new(b"not a zip file".to_vec()),
            Path::new("pack.zip"),
        );

        assert!(matches!(result, Err(Error::NotZip { .. })));
    }

    /// d-dpzip-3
    #[test]
    fn integer_pair_and_integer_are_written_as_declared() {
        assert_eq!(
            written_bounds(r#"{"min_format": [121, 0], "max_format": 121}"#),
            ("121.0".to_owned(), "121".to_owned())
        );
    }

    /// d-dpzip-4
    #[test]
    fn supported_formats_mapping_gives_the_bounds_without_min_and_max_format() {
        assert_eq!(
            written_bounds(
                r#"{"pack_format": 71, "supported_formats": {"min_inclusive": 71, "max_inclusive": 9999}}"#
            ),
            ("71".to_owned(), "9999".to_owned())
        );
    }

    /// 「データパックのファイルの宣言の読み取り」 方法 1: `min_format` and `max_format` come before
    /// `supported_formats` and `pack_format`.
    #[test]
    fn min_and_max_format_come_first() {
        assert_eq!(
            declaration_of_pack(
                r#"{"pack_format": 48, "supported_formats": [48, 57], "min_format": 71, "max_format": [81, 2]}"#
            )
            .unwrap(),
            Some(FormatRange {
                min: FormatNumber::Major(71),
                max: FormatNumber::MajorMinor(81, 2),
            })
        );
    }

    /// 「データパックのファイルの宣言の読み取り」 方法 2: an array of `supported_formats` gives its
    /// two integers.
    #[test]
    fn supported_formats_array_gives_its_integers() {
        assert_eq!(
            written_bounds(r#"{"pack_format": 48, "supported_formats": [48, 57]}"#),
            ("48".to_owned(), "57".to_owned())
        );
    }

    /// 「データパックのファイルの宣言の読み取り」 方法 2: an integer `supported_formats` is both
    /// bounds.
    #[test]
    fn supported_formats_integer_is_both_bounds() {
        assert_eq!(
            written_bounds(r#"{"supported_formats": 57}"#),
            ("57".to_owned(), "57".to_owned())
        );
    }

    /// 「データパックのファイルの宣言の読み取り」 方法 3: `pack_format` alone is both bounds.
    #[test]
    fn pack_format_alone_is_both_bounds() {
        assert_eq!(
            written_bounds(r#"{"pack_format": 48, "description": "A pack"}"#),
            ("48".to_owned(), "48".to_owned())
        );
    }

    /// dinsp-2 `unknown`: a declaration with none of the keys declares no format numbers.
    #[test]
    fn declaration_without_format_keys_declares_none() {
        assert_eq!(
            declaration_of_pack(r#"{"description": ["A", "pack"]}"#).unwrap(),
            None
        );
    }

    /// dinsp-4: only one of `min_format` and `max_format` is an error.
    #[test]
    fn only_one_of_min_and_max_format_is_an_error() {
        assert!(matches!(
            declaration_of_pack(r#"{"pack_format": 71, "min_format": 71}"#),
            Err(Error::OneFormatBound { .. })
        ));
    }

    /// 「データパックのファイルの宣言の読み取り」 方法: a value of the wrong type is an error.
    #[test]
    fn value_of_the_wrong_type_is_an_error() {
        let invalid: Vec<_> = [
            r#"{"pack_format": "71"}"#,
            r#"{"min_format": [121], "max_format": 121}"#,
            r#"{"min_format": [121, 0, 1], "max_format": 121}"#,
            r#"{"supported_formats": {"min_inclusive": 71}}"#,
        ]
        .into_iter()
        .filter(|pack| {
            !matches!(
                declaration_of_pack(pack),
                Err(Error::InvalidPackMcmeta { .. })
            )
        })
        .collect();
        assert!(invalid.is_empty(), "{invalid:?}");
    }

    /// 「データパックのファイルの宣言の読み取り」 方法: a `pack.mcmeta` without `pack` is an error.
    #[test]
    fn pack_mcmeta_without_pack_is_an_error() {
        assert!(matches!(
            declaration_of(&[("pack.mcmeta", br#"{"overlays": {}}"#)]),
            Err(Error::InvalidPackMcmeta { .. })
        ));
    }

    /// 「データパックのファイルの宣言の読み取り」 方法: a `pack.mcmeta` that is not UTF-8 is an
    /// error.
    #[test]
    fn pack_mcmeta_that_is_not_utf8_is_an_error() {
        assert!(matches!(
            declaration_of(&[("pack.mcmeta", b"{\"pack\": {\"description\": \"\xff\"}}")]),
            Err(Error::NotUtf8 { .. })
        ));
    }

    /// 「データパックのファイルの宣言の読み取り」 方法: a leading byte order mark is removed.
    #[test]
    fn leading_byte_order_mark_is_removed() {
        assert_eq!(
            declaration_of(&[(
                "pack.mcmeta",
                "\u{feff}{\"pack\": {\"pack_format\": 71}}".as_bytes()
            )])
            .unwrap(),
            Some(FormatRange {
                min: FormatNumber::Major(71),
                max: FormatNumber::Major(71),
            })
        );
    }
}
