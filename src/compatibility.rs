use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::minecraft_version::{self, MinecraftVersion};
use crate::plugin_cache::{CachedJar, ModrinthRecord};
use crate::plugin_jar::Declaration;
use crate::plugin_source;

const FOLIA: &str = "folia";
const PAPER: &str = "paper";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the api-version {api_version:?} that {} declares is not a Minecraft version mism can compare",
        path.display()
    )]
    MinimumMinecraftVersion {
        api_version: String,
        path: PathBuf,
        #[source]
        source: minecraft_version::Error,
    },
    #[error("the Minecraft version of the server cannot be compared")]
    ServerMinecraftVersion(#[source] minecraft_version::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::MinimumMinecraftVersion { .. } | Error::ServerMinecraftVersion(_) => None,
        }
    }
}

/// The server software (`paper` or `folia`) and the Minecraft version of the target server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetServer<'a> {
    pub software: &'a str,
    pub minecraft_version: &'a str,
}

/// The result of holding the supported server software and Minecraft versions against the target
/// server (vers-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    Match,
    SoftwareOnly,
    Mismatch,
    Unknown,
}

impl fmt::Display for Agreement {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Agreement::Match => "match",
            Agreement::SoftwareOnly => "software-only",
            Agreement::Mismatch => "mismatch",
            Agreement::Unknown => "unknown",
        })
    }
}

/// Holds the supported server software (unknown when empty) and Minecraft versions against the
/// target server, comparing whole strings (vers-3).
pub fn agreement(
    software: &[impl AsRef<str>],
    minecraft_versions: &[impl AsRef<str>],
    target: &TargetServer,
) -> Agreement {
    if software.is_empty() {
        Agreement::Unknown
    } else if !contains(software, target.software) {
        Agreement::Mismatch
    } else if contains(minecraft_versions, target.minecraft_version) {
        Agreement::Match
    } else {
        Agreement::SoftwareOnly
    }
}

fn contains(list: &[impl AsRef<str>], value: &str) -> bool {
    list.iter().any(|element| element.as_ref() == value)
}

/// The newest of the Minecraft versions Modrinth states, leaving out the ones mism cannot parse;
/// the first of equally new ones (srch-2, vers-2).
pub fn newest_minecraft_version(versions: &[String]) -> Option<&str> {
    let mut newest: Option<(MinecraftVersion, &str)> = None;
    for text in versions {
        let Ok(version) = MinecraftVersion::parse(text) else {
            continue;
        };
        if newest
            .as_ref()
            .is_none_or(|(current, _)| version > *current)
        {
            newest = Some((version, text));
        }
    }
    newest.map(|(_, text)| text)
}

/// Where a value came from: the jar's declaration, Modrinth's statements, both, or neither.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sources {
    pub jar: bool,
    pub modrinth: bool,
}

impl Sources {
    const NONE: Sources = Sources {
        jar: false,
        modrinth: false,
    };
    const JAR: Sources = Sources {
        jar: true,
        modrinth: false,
    };
    const MODRINTH: Sources = Sources {
        jar: false,
        modrinth: true,
    };

    fn union(self, other: Sources) -> Sources {
        Sources {
            jar: self.jar || other.jar,
            modrinth: self.modrinth || other.modrinth,
        }
    }

    /// `jar`, `modrinth`, or both: the elements of the provenance (insp-2). None for an unknown
    /// value, which is written `-`.
    pub fn names(self) -> Vec<&'static str> {
        [(self.jar, "jar"), (self.modrinth, "modrinth")]
            .into_iter()
            .filter_map(|(present, name)| present.then_some(name))
            .collect()
    }
}

/// An element of a list value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedElement {
    pub value: String,
    pub sources: Sources,
}

/// A list value of insp-2: elements merged from the declaration and Modrinth's statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedList {
    /// In ascending order, each once.
    pub elements: Vec<SourcedElement>,
    /// The sources that state this list even when it has no elements.
    stated_by: Sources,
}

impl SourcedList {
    fn merge(
        from_jar: impl IntoIterator<Item = String>,
        from_modrinth: impl IntoIterator<Item = String>,
        stated_by: Sources,
    ) -> SourcedList {
        let mut merged: BTreeMap<String, Sources> = BTreeMap::new();
        for (values, sources) in [
            (from_jar.into_iter().collect::<Vec<_>>(), Sources::JAR),
            (from_modrinth.into_iter().collect(), Sources::MODRINTH),
        ] {
            for value in values {
                let entry = merged.entry(value).or_default();
                *entry = entry.union(sources);
            }
        }
        SourcedList {
            elements: merged
                .into_iter()
                .map(|(value, sources)| SourcedElement { value, sources })
                .collect(),
            stated_by,
        }
    }

    pub fn values(&self) -> Vec<&str> {
        self.elements
            .iter()
            .map(|element| element.value.as_str())
            .collect()
    }

    /// The sources of the elements together, or for no elements the sources that state the list
    /// (insp-2). `Sources::NONE` means the value is unknown.
    pub fn sources(&self) -> Sources {
        if self.elements.is_empty() {
            self.stated_by
        } else {
            self.elements
                .iter()
                .fold(Sources::NONE, |sources, element| {
                    sources.union(element.sources)
                })
        }
    }

    pub fn is_unknown(&self) -> bool {
        self.sources() == Sources::NONE
    }
}

/// The six `compatibility` values of insp-2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compatibility {
    /// The declaration's `api-version` (from `jar`); `None` when unknown.
    pub minimum_minecraft_version: Option<String>,
    /// Modrinth's `game_versions` in its order (from `modrinth`); `None` when unknown.
    pub minecraft_versions: Option<Vec<String>>,
    pub software: SourcedList,
    /// From `jar`.
    pub folia_supported: bool,
    pub required_dependencies: SourcedList,
    pub optional_dependencies: SourcedList,
}

/// The 13 values of the inspection description (insp-2) of a jar in the cache directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    pub name: String,
    /// The Modrinth version number; `None` unless the source is Modrinth.
    pub version: Option<String>,
    pub source: String,
    pub file_name: String,
    pub sha256: String,
    /// The absolute path of the jar in the cache directory.
    pub path: PathBuf,
    pub declaration: Declaration,
    pub modrinth: Option<ModrinthRecord>,
    pub compatibility: Compatibility,
}

/// The inspection values of `cached_jar`, whose declaration is `declaration` (insp-2).
pub fn inspect(cached_jar: &CachedJar, declaration: Declaration) -> Inspection {
    Inspection {
        name: cached_jar.name.clone(),
        version: plugin_source::version_number_of(&cached_jar.source).map(str::to_owned),
        source: cached_jar.source.clone(),
        file_name: cached_jar.file_name.clone(),
        sha256: cached_jar.sha256.clone(),
        path: cached_jar.path.clone(),
        compatibility: compatibility(&declaration, cached_jar.modrinth.as_ref()),
        declaration,
        modrinth: cached_jar.modrinth.clone(),
    }
}

fn compatibility(declaration: &Declaration, modrinth: Option<&ModrinthRecord>) -> Compatibility {
    Compatibility {
        minimum_minecraft_version: declaration.api_version.clone(),
        minecraft_versions: modrinth
            .map(|modrinth| modrinth.game_versions.clone())
            .filter(|game_versions| !game_versions.is_empty()),
        software: software(declaration, modrinth),
        folia_supported: declaration.folia_supported,
        required_dependencies: SourcedList::merge(
            declaration.required_dependencies.iter().cloned(),
            modrinth
                .into_iter()
                .flat_map(|modrinth| modrinth.required_dependencies.iter().cloned()),
            // The declaration always states its dependencies, Modrinth when it is the source.
            Sources::JAR.union(if modrinth.is_some() {
                Sources::MODRINTH
            } else {
                Sources::NONE
            }),
        ),
        optional_dependencies: SourcedList::merge(
            declaration.optional_dependencies.iter().cloned(),
            [],
            Sources::JAR,
        ),
    }
}

/// `folia` when the declaration declares Folia support, `paper` for the Paper plugin format, and
/// Modrinth's `loaders`. A source that adds no element states nothing about this list.
fn software(declaration: &Declaration, modrinth: Option<&ModrinthRecord>) -> SourcedList {
    let from_jar: Vec<String> = [
        (declaration.folia_supported, FOLIA),
        (declaration.paper_plugin_format, PAPER),
    ]
    .into_iter()
    .filter(|(declared, _)| *declared)
    .map(|(_, software)| software.to_owned())
    .collect();
    let from_modrinth: Vec<String> = modrinth
        .map(|modrinth| modrinth.loaders.clone())
        .unwrap_or_default();
    SourcedList::merge(from_jar, from_modrinth, Sources::NONE)
}

/// What is directly in the target server's `plugins/` and recorded in its `mism.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstalledPlugins<'a> {
    /// The file name of each jar directly in `plugins/`, and the plugin name its declaration
    /// declares.
    pub jars: Vec<(&'a str, Option<&'a str>)>,
    /// The identifier of the source of each record that has one.
    pub record_identifiers: Vec<&'a str>,
}

/// The judgements of insp-3. A judgement whose list is empty does not apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    pub declared: Agreement,
    pub incompatible: Vec<String>,
    pub missing: Vec<String>,
    pub conflict: Vec<String>,
}

impl Judgement {
    /// `incompatible` or `conflict` stops placing the jar (place-6, inst-4).
    pub fn prevents_placement(&self) -> bool {
        !self.incompatible.is_empty() || !self.conflict.is_empty()
    }
}

/// The judgements of insp-3 for `inspection` on the target server.
pub fn judge(
    inspection: &Inspection,
    target: &TargetServer,
    installed: &InstalledPlugins,
) -> Result<Judgement, Error> {
    let compatibility = &inspection.compatibility;
    Ok(Judgement {
        declared: agreement(
            &compatibility.software.values(),
            compatibility
                .minecraft_versions
                .as_deref()
                .unwrap_or_default(),
            target,
        ),
        incompatible: incompatibility_reasons(inspection, target)?,
        missing: missing_dependencies(&compatibility.required_dependencies, installed),
        conflict: conflicting_jars(inspection.declaration.name.as_deref(), installed),
    })
}

/// Step 2.
fn incompatibility_reasons(
    inspection: &Inspection,
    target: &TargetServer,
) -> Result<Vec<String>, Error> {
    let compatibility = &inspection.compatibility;
    let mut reasons = Vec::new();
    if let Some(minimum) = &compatibility.minimum_minecraft_version
        && requires_newer_minecraft(minimum, inspection, target)?
    {
        reasons.push(format!("requires Minecraft {minimum} or newer"));
    }
    if target.software == FOLIA && !compatibility.folia_supported {
        reasons.push("does not declare Folia support".to_owned());
    }
    Ok(reasons)
}

fn requires_newer_minecraft(
    minimum: &str,
    inspection: &Inspection,
    target: &TargetServer,
) -> Result<bool, Error> {
    let minimum_version =
        MinecraftVersion::parse(minimum).map_err(|source| Error::MinimumMinecraftVersion {
            api_version: minimum.to_owned(),
            path: inspection.path.clone(),
            source,
        })?;
    let server_version =
        MinecraftVersion::parse(target.minecraft_version).map_err(Error::ServerMinecraftVersion)?;
    Ok(minimum_version > server_version)
}

/// Step 3: an element from the declaration is present when a jar in `plugins/` declares that
/// name, an element from Modrinth when a record's source has that identifier.
fn missing_dependencies(required: &SourcedList, installed: &InstalledPlugins) -> Vec<String> {
    required
        .elements
        .iter()
        .filter(|element| {
            let declared_by_a_jar = element.sources.jar
                && installed
                    .jars
                    .iter()
                    .any(|(_, name)| *name == Some(element.value.as_str()));
            let recorded = element.sources.modrinth
                && installed
                    .record_identifiers
                    .contains(&element.value.as_str());
            !declared_by_a_jar && !recorded
        })
        .map(|element| element.value.clone())
        .collect()
}

/// Step 4: the jars in `plugins/` that declare the same plugin name, none without a name.
fn conflicting_jars(name: Option<&str>, installed: &InstalledPlugins) -> Vec<String> {
    let Some(name) = name else {
        return Vec::new();
    };
    installed
        .jars
        .iter()
        .filter(|(_, declared)| *declared == Some(name))
        .map(|(file_name, _)| (*file_name).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::*;

    const PAPER_26_3: TargetServer = TargetServer {
        software: "paper",
        minecraft_version: "26.3",
    };
    const FOLIA_26_3: TargetServer = TargetServer {
        software: "folia",
        minecraft_version: "26.3",
    };

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn names(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn cached_from_url() -> CachedJar {
        CachedJar {
            name: "Example".to_owned(),
            source: "https://example.com/Example.jar".to_owned(),
            file_name: "Example.jar".to_owned(),
            sha256: "0".repeat(64),
            path: PathBuf::from("/cache/mism/plugins/Example/0/Example.jar"),
            modrinth: None,
        }
    }

    fn cached_from_modrinth(modrinth: ModrinthRecord) -> CachedJar {
        CachedJar {
            name: "LuckPerms".to_owned(),
            source: "modrinth:luckperms:v5.5.71-bukkit".to_owned(),
            file_name: "LuckPerms-Bukkit-5.5.71.jar".to_owned(),
            modrinth: Some(modrinth),
            ..cached_from_url()
        }
    }

    fn record(loaders: &[&str], game_versions: &[&str], required: &[&str]) -> ModrinthRecord {
        ModrinthRecord {
            loaders: strings(loaders),
            game_versions: strings(game_versions),
            required_dependencies: strings(required),
        }
    }

    fn declaration_named(name: &str) -> Declaration {
        Declaration {
            name: Some(name.to_owned()),
            ..Declaration::default()
        }
    }

    fn values_and_sources(list: &SourcedList) -> (Vec<&str>, Vec<&'static str>) {
        (list.values(), list.sources().names())
    }

    /// d-compat-1
    #[test]
    fn url_jar_without_depend_has_no_required_dependency_from_the_jar() {
        let inspection = inspect(&cached_from_url(), declaration_named("Example"));

        assert_eq!(
            values_and_sources(&inspection.compatibility.required_dependencies),
            (vec![], vec!["jar"])
        );
    }

    /// d-compat-2
    #[test]
    fn dependency_from_both_sources_appears_once() {
        let declaration = Declaration {
            required_dependencies: names(&["Vault"]),
            ..declaration_named("Example")
        };
        let inspection = inspect(
            &cached_from_modrinth(record(&["paper"], &["26.3"], &["Vault"])),
            declaration,
        );

        assert_eq!(
            values_and_sources(&inspection.compatibility.required_dependencies),
            (vec!["Vault"], vec!["jar", "modrinth"])
        );
    }

    /// d-compat-3
    #[test]
    fn unparsable_api_version_with_a_target_server_is_an_error() {
        let declaration = Declaration {
            api_version: Some("next".to_owned()),
            ..declaration_named("Example")
        };
        let inspection = inspect(&cached_from_url(), declaration);

        let error = judge(&inspection, &PAPER_26_3, &InstalledPlugins::default()).unwrap_err();

        let message = error.to_string();
        assert!(
            message.contains("next") && message.contains(&inspection.path.display().to_string()),
            "{message}"
        );
    }

    /// d-compat-4: without a target server, an unparsable `api-version` is the value as written.
    #[test]
    fn unparsable_api_version_without_a_target_server_is_the_value() {
        let declaration = Declaration {
            api_version: Some("next".to_owned()),
            ..declaration_named("Example")
        };

        let inspection = inspect(&cached_from_url(), declaration);

        assert_eq!(
            inspection
                .compatibility
                .minimum_minecraft_version
                .as_deref(),
            Some("next")
        );
    }

    /// d-compat-5
    #[test]
    fn newest_minecraft_version_includes_snapshots_and_skips_unparsable_ones() {
        assert_eq!(
            newest_minecraft_version(&strings(&["26.3", "26.4-snapshot-5", "25w46a"])),
            Some("26.4-snapshot-5")
        );
    }

    /// d-compat-6
    #[test]
    fn no_parsable_minecraft_version_has_no_newest() {
        assert_eq!(
            newest_minecraft_version(&strings(&["25w46a", "b1.7.3"])),
            None
        );
    }

    /// 「検証の値と判定」 方法: of equally new versions the first is the newest.
    #[test]
    fn first_of_equally_new_versions_is_the_newest() {
        assert_eq!(
            newest_minecraft_version(&strings(&["1.21", "1.21.0"])),
            Some("1.21")
        );
    }

    /// insp-2: `version` is the Modrinth version number.
    #[test]
    fn version_is_the_version_number_of_a_modrinth_source() {
        let inspection = inspect(
            &cached_from_modrinth(record(&["paper"], &["26.3"], &[])),
            declaration_named("LuckPerms"),
        );

        assert_eq!(inspection.version.as_deref(), Some("v5.5.71-bukkit"));
    }

    /// insp-2: `version` is absent unless the source is Modrinth.
    #[test]
    fn version_is_absent_for_a_url_source() {
        assert_eq!(
            inspect(&cached_from_url(), Declaration::default()).version,
            None
        );
    }

    /// insp-2: `minimum_minecraft_version` is unknown without `api-version`.
    #[test]
    fn minimum_minecraft_version_is_unknown_without_api_version() {
        let inspection = inspect(&cached_from_url(), Declaration::default());

        assert_eq!(inspection.compatibility.minimum_minecraft_version, None);
    }

    /// insp-2: `minecraft_versions` is Modrinth's `game_versions` in its order.
    #[test]
    fn minecraft_versions_are_modrinth_game_versions_in_order() {
        let inspection = inspect(
            &cached_from_modrinth(record(&["paper"], &["26.3", "1.9"], &[])),
            Declaration::default(),
        );

        assert_eq!(
            inspection.compatibility.minecraft_versions,
            Some(strings(&["26.3", "1.9"]))
        );
    }

    /// insp-2: `minecraft_versions` is unknown for a URL source and for empty `game_versions`.
    #[test]
    fn minecraft_versions_are_unknown_without_modrinth_game_versions() {
        let known: Vec<_> = [
            cached_from_url(),
            cached_from_modrinth(record(&["paper"], &[], &[])),
        ]
        .iter()
        .filter_map(|cached| {
            inspect(cached, Declaration::default())
                .compatibility
                .minecraft_versions
        })
        .collect();
        assert!(known.is_empty(), "{known:?}");
    }

    /// insp-2: the declared Folia support and Modrinth's loaders make the software list, with
    /// where each came from.
    #[test]
    fn software_merges_the_declaration_and_modrinth() {
        let declaration = Declaration {
            folia_supported: true,
            ..Declaration::default()
        };
        let inspection = inspect(
            &cached_from_modrinth(record(&["paper", "folia"], &["26.3"], &[])),
            declaration,
        );

        let elements: Vec<_> = inspection
            .compatibility
            .software
            .elements
            .iter()
            .map(|element| (element.value.as_str(), element.sources.names()))
            .collect();
        assert_eq!(
            elements,
            [
                ("folia", vec!["jar", "modrinth"]),
                ("paper", vec!["modrinth"])
            ]
        );
    }

    /// insp-2: the Paper plugin format adds `paper` from the jar.
    #[test]
    fn paper_plugin_format_adds_paper() {
        let declaration = Declaration {
            paper_plugin_format: true,
            ..Declaration::default()
        };

        let inspection = inspect(&cached_from_url(), declaration);

        assert_eq!(
            values_and_sources(&inspection.compatibility.software),
            (vec!["paper"], vec!["jar"])
        );
    }

    /// insp-2: software that no source tells is unknown.
    #[test]
    fn software_is_unknown_without_any_statement() {
        let inspection = inspect(&cached_from_url(), Declaration::default());

        assert!(inspection.compatibility.software.is_unknown());
    }

    /// insp-2: optional dependencies come from the jar even when there are none.
    #[test]
    fn optional_dependencies_come_from_the_jar() {
        let inspection = inspect(
            &cached_from_modrinth(record(&["paper"], &["26.3"], &[])),
            Declaration::default(),
        );

        assert_eq!(
            values_and_sources(&inspection.compatibility.optional_dependencies),
            (vec![], vec!["jar"])
        );
    }

    /// vers-3: `match`.
    #[test]
    fn listed_software_and_minecraft_version_match() {
        assert_eq!(
            agreement(&["bukkit", "paper"], &["1.9", "26.3"], &PAPER_26_3),
            Agreement::Match
        );
    }

    /// vers-3: `software-only`.
    #[test]
    fn listed_software_without_the_minecraft_version_is_software_only() {
        assert_eq!(
            agreement(&["paper"], &["26.2"], &PAPER_26_3),
            Agreement::SoftwareOnly
        );
    }

    /// vers-3: `mismatch`.
    #[test]
    fn unlisted_software_is_a_mismatch() {
        assert_eq!(
            agreement(&["paper"], &["26.3"], &FOLIA_26_3),
            Agreement::Mismatch
        );
    }

    /// vers-3: `unknown`.
    #[test]
    fn unknown_software_is_unknown() {
        assert_eq!(
            agreement(&[] as &[&str], &["26.3"], &PAPER_26_3),
            Agreement::Unknown
        );
    }

    /// 「検証の値と判定」 方法: lists contain a value only as the whole string.
    #[test]
    fn minecraft_version_is_compared_as_a_whole_string() {
        assert_eq!(
            agreement(&["paper"], &["26.3.0"], &PAPER_26_3),
            Agreement::SoftwareOnly
        );
    }

    /// insp-3: `declared` holds the inspection's software and Minecraft versions against the
    /// server.
    #[test]
    fn declared_is_the_agreement_of_the_inspection() {
        let inspection = inspect(
            &cached_from_modrinth(record(&["paper"], &["26.3"], &[])),
            declaration_named("LuckPerms"),
        );

        let judgement = judge(&inspection, &PAPER_26_3, &InstalledPlugins::default()).unwrap();

        assert_eq!(judgement.declared, Agreement::Match);
    }

    /// insp-3: a minimum Minecraft version newer than the server's is `incompatible`.
    #[test]
    fn newer_minimum_minecraft_version_is_incompatible() {
        let declaration = Declaration {
            api_version: Some("26.4".to_owned()),
            ..declaration_named("Example")
        };
        let inspection = inspect(&cached_from_url(), declaration);

        let judgement = judge(&inspection, &PAPER_26_3, &InstalledPlugins::default()).unwrap();

        assert_eq!(judgement.incompatible, ["requires Minecraft 26.4 or newer"]);
    }

    /// insp-3: an older minimum Minecraft version is not `incompatible`.
    #[test]
    fn older_minimum_minecraft_version_is_compatible() {
        let declaration = Declaration {
            api_version: Some("1.20".to_owned()),
            ..declaration_named("Example")
        };
        let inspection = inspect(&cached_from_url(), declaration);

        let judgement = judge(&inspection, &PAPER_26_3, &InstalledPlugins::default()).unwrap();

        assert_eq!(judgement.incompatible, Vec::<String>::new());
    }

    /// insp-3: on a Folia server, a jar without declared Folia support is `incompatible`.
    #[test]
    fn folia_server_without_declared_support_is_incompatible() {
        let inspection = inspect(&cached_from_url(), declaration_named("Example"));

        let judgement = judge(&inspection, &FOLIA_26_3, &InstalledPlugins::default()).unwrap();

        assert_eq!(judgement.incompatible, ["does not declare Folia support"]);
    }

    /// insp-3: a dependency from the jar is present when a jar in `plugins/` declares its name.
    #[test]
    fn dependency_declared_by_a_jar_in_plugins_is_not_missing() {
        let declaration = Declaration {
            required_dependencies: names(&["Vault", "ProtocolLib"]),
            ..declaration_named("Example")
        };
        let inspection = inspect(&cached_from_url(), declaration);
        let installed = InstalledPlugins {
            jars: vec![("Vault.jar", Some("Vault"))],
            record_identifiers: vec!["protocollib"],
        };

        let judgement = judge(&inspection, &PAPER_26_3, &installed).unwrap();

        assert_eq!(judgement.missing, ["ProtocolLib"]);
    }

    /// insp-3: a dependency from Modrinth is present when a record's source has its identifier.
    #[test]
    fn dependency_recorded_with_its_identifier_is_not_missing() {
        let inspection = inspect(
            &cached_from_modrinth(record(&["paper"], &["26.3"], &["multiverse-core", "vault"])),
            declaration_named("Multiverse-Portals"),
        );
        let installed = InstalledPlugins {
            jars: vec![("vault.jar", Some("vault"))],
            record_identifiers: vec!["multiverse-core"],
        };

        let judgement = judge(&inspection, &PAPER_26_3, &installed).unwrap();

        assert_eq!(judgement.missing, ["vault"]);
    }

    /// insp-3: jars in `plugins/` that declare the same name are a `conflict`.
    #[test]
    fn jar_declaring_the_same_name_is_a_conflict() {
        let inspection = inspect(&cached_from_url(), declaration_named("Example"));
        let installed = InstalledPlugins {
            jars: vec![
                ("Other.jar", Some("Other")),
                ("Example-old.jar", Some("Example")),
            ],
            record_identifiers: Vec::new(),
        };

        let judgement = judge(&inspection, &PAPER_26_3, &installed).unwrap();

        assert_eq!(judgement.conflict, ["Example-old.jar"]);
    }

    /// 「検証の値と判定」 方法 4: without a declared name there is no `conflict`.
    #[test]
    fn jar_without_a_name_has_no_conflict() {
        let inspection = inspect(&cached_from_url(), Declaration::default());
        let installed = InstalledPlugins {
            jars: vec![("Nameless.jar", None)],
            record_identifiers: Vec::new(),
        };

        let judgement = judge(&inspection, &PAPER_26_3, &installed).unwrap();

        assert_eq!(judgement.conflict, Vec::<String>::new());
    }

    /// place-6: `conflict` prevents placing the jar.
    #[test]
    fn conflict_prevents_placement() {
        let judgement = Judgement {
            declared: Agreement::Match,
            incompatible: Vec::new(),
            missing: vec!["Vault".to_owned()],
            conflict: vec!["Example.jar".to_owned()],
        };

        assert!(judgement.prevents_placement());
    }

    /// place-6: `missing` alone does not prevent placing the jar.
    #[test]
    fn missing_alone_does_not_prevent_placement() {
        let judgement = Judgement {
            declared: Agreement::Unknown,
            incompatible: Vec::new(),
            missing: vec!["Vault".to_owned()],
            conflict: Vec::new(),
        };

        assert!(!judgement.prevents_placement());
    }
}
