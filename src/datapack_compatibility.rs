use std::fmt;

use crate::datapack_zip::{FormatNumber, FormatRange};
use crate::fetched_file_cache::CachedFile;

/// The result of holding a datapack against the target server (dvers-3, dinsp-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    Match,
    Mismatch,
    Unknown,
}

impl fmt::Display for Agreement {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Agreement::Match => "match",
            Agreement::Mismatch => "mismatch",
            Agreement::Unknown => "unknown",
        })
    }
}

/// Holds a datapack's supported Minecraft versions against the target server's, comparing whole
/// strings (dvers-3).
pub fn agreement(minecraft_versions: &[impl AsRef<str>], minecraft_version: &str) -> Agreement {
    if minecraft_versions
        .iter()
        .any(|element| element.as_ref() == minecraft_version)
    {
        Agreement::Match
    } else {
        Agreement::Mismatch
    }
}

/// The 9 values of the inspection description (dinsp-2) of a datapack file in the cache
/// directory: those of the file itself, with Modrinth's statements, and the format numbers its
/// `pack.mcmeta` declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    pub file: CachedFile,
    /// `None` when unknown.
    pub declaration: Option<FormatRange>,
}

/// The inspection values of `cached_file`, whose `pack.mcmeta` declares `declaration` (dinsp-2).
pub fn inspect(cached_file: &CachedFile, declaration: Option<FormatRange>) -> Inspection {
    Inspection {
        file: cached_file.clone(),
        declaration,
    }
}

/// The Minecraft version of the target server and the major and minor number of its data pack
/// format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target<'a> {
    pub minecraft_version: &'a str,
    pub format_major: u32,
    pub format_minor: u32,
}

/// The judgements of dinsp-3. `missing` does not apply when it is empty. None of them stops
/// placing the datapack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    pub declared: Agreement,
    pub pack_format: Agreement,
    pub missing: Vec<String>,
}

/// The judgements of dinsp-3 for `inspection` on the target server, whose `mism.toml` records
/// sources with `record_identifiers`, those of plugins and of datapacks.
pub fn judge(inspection: &Inspection, target: &Target, record_identifiers: &[&str]) -> Judgement {
    let modrinth = inspection.file.modrinth.as_ref();
    Judgement {
        declared: modrinth.map_or(Agreement::Unknown, |modrinth| {
            agreement(&modrinth.game_versions, target.minecraft_version)
        }),
        pack_format: format_agreement(inspection.declaration.as_ref(), target),
        missing: modrinth.map_or_else(Vec::new, |modrinth| {
            modrinth
                .required_dependencies
                .iter()
                .filter(|identifier| !record_identifiers.contains(&identifier.as_str()))
                .cloned()
                .collect()
        }),
    }
}

/// Step 2 of the judgements: whether the server's format number is within the declared bounds,
/// both included.
fn format_agreement(range: Option<&FormatRange>, target: &Target) -> Agreement {
    let Some(range) = range else {
        return Agreement::Unknown;
    };
    let server = (target.format_major, target.format_minor);
    if lowest_of(range.min) <= server && is_at_most(server, range.max) {
        Agreement::Match
    } else {
        Agreement::Mismatch
    }
}

/// An integer lower bound is that major number with the minor number 0.
fn lowest_of(min: FormatNumber) -> (u32, u32) {
    match min {
        FormatNumber::Major(major) => (major, 0),
        FormatNumber::MajorMinor(major, minor) => (major, minor),
    }
}

/// An integer upper bound includes every minor number of that major number.
fn is_at_most(server: (u32, u32), max: FormatNumber) -> bool {
    match max {
        FormatNumber::Major(major) => server.0 <= major,
        FormatNumber::MajorMinor(major, minor) => server <= (major, minor),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::fetched_file_cache::ModrinthRecord;

    const DATAPACK_26_3: Target = Target {
        minecraft_version: "26.3",
        format_major: 121,
        format_minor: 0,
    };

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn cached_from_url() -> CachedFile {
        CachedFile {
            name: "Example".to_owned(),
            source: "https://example.com/Example.zip".to_owned(),
            file_name: "Example.zip".to_owned(),
            sha256: "0".repeat(64),
            path: PathBuf::from("/cache/mism/datapacks/Example/0/Example.zip"),
            modrinth: None,
        }
    }

    fn true_ending_from_modrinth(required: &[&str]) -> CachedFile {
        CachedFile {
            name: "True Ending - Ender Dragon Overhaul".to_owned(),
            source: "modrinth:true-ending:1.1.4d".to_owned(),
            file_name: "tru.e-ending-1.1.4d.zip".to_owned(),
            modrinth: Some(ModrinthRecord {
                loaders: strings(&["datapack"]),
                game_versions: strings(&["26.1.2", "26.3"]),
                required_dependencies: strings(required),
            }),
            ..cached_from_url()
        }
    }

    fn range(min: FormatNumber, max: FormatNumber) -> Option<FormatRange> {
        Some(FormatRange { min, max })
    }

    fn pack_format_on(declaration: Option<FormatRange>, target: &Target) -> Agreement {
        judge(&inspect(&cached_from_url(), declaration), target, &[]).pack_format
    }

    /// dinsp-3: a required dependency among the identifiers of the records is not `missing`.
    #[test]
    fn datapack_dependency_among_the_record_identifiers_is_not_missing() {
        let inspection = inspect(
            &true_ending_from_modrinth(&["stellarity", "luckperms"]),
            None,
        );

        let judgement = judge(&inspection, &DATAPACK_26_3, &["luckperms"]);

        assert_eq!(judgement.missing, ["stellarity"]);
    }

    /// d-compat-8
    #[test]
    fn integer_bounds_include_minor_zero_of_their_major_number() {
        assert_eq!(
            pack_format_on(
                range(FormatNumber::Major(121), FormatNumber::Major(121)),
                &DATAPACK_26_3
            ),
            Agreement::Match
        );
    }

    /// d-compat-9
    #[test]
    fn lower_bound_with_a_minor_number_excludes_lower_minor_numbers() {
        assert_eq!(
            pack_format_on(
                range(FormatNumber::MajorMinor(121, 1), FormatNumber::Major(121)),
                &DATAPACK_26_3
            ),
            Agreement::Mismatch
        );
    }

    /// d-compat-10
    #[test]
    fn integer_upper_bound_includes_every_minor_number() {
        let target = Target {
            format_major: 120,
            format_minor: 5,
            ..DATAPACK_26_3
        };

        assert_eq!(
            pack_format_on(
                range(FormatNumber::Major(100), FormatNumber::Major(120)),
                &target
            ),
            Agreement::Match
        );
    }

    /// dinsp-3: an upper bound with a minor number excludes higher minor numbers.
    #[test]
    fn upper_bound_with_a_minor_number_excludes_higher_minor_numbers() {
        let target = Target {
            format_minor: 2,
            ..DATAPACK_26_3
        };

        assert_eq!(
            pack_format_on(
                range(FormatNumber::Major(100), FormatNumber::MajorMinor(121, 1)),
                &target
            ),
            Agreement::Mismatch
        );
    }

    /// dinsp-3: a higher major number than the upper bound is a `mismatch`.
    #[test]
    fn major_number_above_the_upper_bound_is_a_mismatch() {
        assert_eq!(
            pack_format_on(
                range(FormatNumber::Major(48), FormatNumber::Major(57)),
                &DATAPACK_26_3
            ),
            Agreement::Mismatch
        );
    }

    /// dinsp-3: without declared format numbers `pack_format` is `unknown`.
    #[test]
    fn pack_format_without_a_declaration_is_unknown() {
        assert_eq!(pack_format_on(None, &DATAPACK_26_3), Agreement::Unknown);
    }

    /// dinsp-3: `declared` holds Modrinth's Minecraft versions against the server's.
    #[test]
    fn datapack_declared_is_the_minecraft_version_agreement() {
        let inspection = inspect(&true_ending_from_modrinth(&[]), None);
        let target = Target {
            minecraft_version: "26.2",
            ..DATAPACK_26_3
        };

        let declared: Vec<_> = [DATAPACK_26_3, target]
            .iter()
            .map(|target| judge(&inspection, target, &[]).declared)
            .collect();

        assert_eq!(declared, [Agreement::Match, Agreement::Mismatch]);
    }

    /// dinsp-3: `declared` is `unknown` unless the source is Modrinth.
    #[test]
    fn datapack_declared_is_unknown_unless_from_modrinth() {
        let inspection = inspect(&cached_from_url(), None);

        assert_eq!(
            judge(&inspection, &DATAPACK_26_3, &[]).declared,
            Agreement::Unknown
        );
    }

    /// dvers-3: the Minecraft versions are compared as whole strings.
    #[test]
    fn datapack_minecraft_versions_are_compared_as_whole_strings() {
        assert_eq!(
            agreement(&["26.1", "26.1.2"], "26.1.1"),
            Agreement::Mismatch
        );
    }
}
