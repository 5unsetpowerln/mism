use std::path::PathBuf;

use super::{CommonStepError, fetch};
use crate::command_line::InspectArguments;
use crate::compatibility::{self, Inspection, InstalledPlugins, Judgement, SourcedList};
use crate::interruption::Signal;
use crate::output::{self, Field};
use crate::plugin_cache::{self, CachedJar};
use crate::plugin_jar::{self, Declaration};
use crate::plugin_source::{self, PluginSource};
use crate::server_directory::ServerDirectory;
use crate::server_directory::plugins::{self, ServerJar};

/// How insp-2 writes a value that neither the declaration nor Modrinth tells.
const UNKNOWN: &str = "unknown";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Source(#[from] plugin_source::Error),
    #[error(transparent)]
    PluginCache(#[from] plugin_cache::Error),
    #[error(transparent)]
    Fetch(#[from] fetch::Error),
    #[error(transparent)]
    Declaration(#[from] plugin_jar::Error),
    #[error(
        "{} has neither plugin.yml nor paper-plugin.yml, so it declares no plugin",
        path.display()
    )]
    NoDeclaration { path: PathBuf },
    #[error(transparent)]
    Plugins(#[from] plugins::Error),
    #[error(transparent)]
    Compatibility(#[from] compatibility::Error),
    #[error(transparent)]
    CommonStep(#[from] CommonStepError),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            // The cached jar could not be read.
            Error::Declaration(
                plugin_jar::Error::ReadJar { path, .. } | plugin_jar::Error::ReadEntry { path, .. },
            ) => Some(format!(
                "remove the directory {} and fetch the plugin again",
                path.parent()
                    .expect("a cached jar is in a SHA-256 directory")
                    .display()
            )),
            Error::Source(error) => error.hint(),
            Error::PluginCache(error) => error.hint(),
            Error::Fetch(error) => error.hint(),
            Error::Declaration(error) => error.hint(),
            Error::Plugins(error) => error.hint(),
            Error::Compatibility(error) => error.hint(),
            Error::CommonStep(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::NoDeclaration { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::PluginCache(error) => error.interrupted_by(),
            Error::Fetch(error) => error.interrupted_by(),
            Error::CommonStep(error) => error.interrupted_by(),
            Error::Source(_)
            | Error::Declaration(_)
            | Error::NoDeclaration { .. }
            | Error::Plugins(_)
            | Error::Compatibility(_)
            | Error::Output(_) => None,
        }
    }
}

/// `mism plugin inspect` (insp-1).
pub fn run(arguments: &InspectArguments) -> Result<(), Error> {
    let server = super::target_server()?;
    let source = plugin_source::parse(&arguments.source)?;
    let cached_jar = match find_in_cache(&arguments.source, &source)? {
        Some(cached_jar) => cached_jar,
        None => fetch::fetch_jar(&arguments.source, &source, arguments.yes, None)?,
    };
    let inspection = write_inspection(&cached_jar)?;
    if let Some(server) = &server {
        write_judgement(&inspection, server)?;
    }
    Ok(())
}

/// Step 2: the jar fetched with the source `text`, or for a local jar the cached jar with its
/// content (place-1, insp-1).
pub fn find_in_cache(text: &str, source: &PluginSource) -> Result<Option<CachedJar>, Error> {
    Ok(match source {
        PluginSource::Modrinth(_) | PluginSource::Url(_) => plugin_cache::find_by_source(text)?,
        PluginSource::File(local_jar) => plugin_cache::find_by_content(&local_jar.path, text)?,
    })
}

/// Steps 4 and 5 of the inspection procedure (検証の手順): the 13 values of insp-2.
pub fn write_inspection(cached_jar: &CachedJar) -> Result<Inspection, Error> {
    let inspection = compatibility::inspect(cached_jar, read_declaration(cached_jar)?);
    output::write_to_stdout(&inspection_lines(&inspection).join("\n"))?;
    Ok(inspection)
}

/// Step 4 (insp-4).
fn read_declaration(cached_jar: &CachedJar) -> Result<Declaration, Error> {
    plugin_jar::read_declaration(&cached_jar.path)?.ok_or_else(|| Error::NoDeclaration {
        path: cached_jar.path.clone(),
    })
}

/// The name and the value of each of the 13 values, and the source of each `compatibility` one
/// (insp-2).
fn inspection_lines(inspection: &Inspection) -> Vec<String> {
    let declaration = &inspection.declaration;
    let compatibility = &inspection.compatibility;
    let folia_supported = compatibility.folia_supported.to_string();
    vec![
        value_line("name", Some(&inspection.name)),
        value_line("version", inspection.version.as_deref()),
        value_line("source", Some(&inspection.source)),
        value_line("file.name", Some(&inspection.file_name)),
        value_line("file.sha256", Some(&inspection.sha256)),
        value_line("declaration.name", declaration.name.as_deref()),
        value_line("declaration.version", declaration.version.as_deref()),
        sourced_line(
            "compatibility.minimum_minecraft_version",
            compatibility
                .minimum_minecraft_version
                .as_deref()
                .map(|version| (Field::Value(Some(version)), vec!["jar"])),
        ),
        sourced_line(
            "compatibility.minecraft_versions",
            compatibility.minecraft_versions.as_ref().map(|versions| {
                (
                    Field::MinecraftVersions(versions.iter().map(String::as_str).collect()),
                    vec!["modrinth"],
                )
            }),
        ),
        list_line("compatibility.software", &compatibility.software),
        sourced_line(
            "compatibility.folia_supported",
            Some((Field::Value(Some(&folia_supported)), vec!["jar"])),
        ),
        list_line(
            "compatibility.required_dependencies",
            &compatibility.required_dependencies,
        ),
        list_line(
            "compatibility.optional_dependencies",
            &compatibility.optional_dependencies,
        ),
    ]
}

fn value_line(name: &str, value: Option<&str>) -> String {
    output::record(&[Field::Value(Some(name)), Field::Value(value)])
}

/// A `compatibility` value and its sources, or `unknown` and `-` when it is `None`.
fn sourced_line(name: &str, value: Option<(Field, Vec<&str>)>) -> String {
    let (value, sources) = value.unwrap_or((Field::Value(Some(UNKNOWN)), Vec::new()));
    output::record(&[Field::Value(Some(name)), value, Field::List(sources)])
}

fn list_line(name: &str, list: &SourcedList) -> String {
    sourced_line(
        name,
        (!list.is_unknown()).then(|| (Field::List(list.values()), list.sources().names())),
    )
}

/// Step 6 of the inspection procedure: the judgements of insp-3 on the target server.
pub fn write_judgement(
    inspection: &Inspection,
    server: &ServerDirectory,
) -> Result<Judgement, Error> {
    let jars = plugins::jars(server)?;
    let judgement = compatibility::judge(
        inspection,
        &super::target_of(server),
        &installed_plugins(server, &jars),
    )?;
    output::write_to_stdout(&judgement_lines(&judgement).join("\n"))?;
    Ok(judgement)
}

fn installed_plugins<'a>(
    server: &'a ServerDirectory,
    jars: &'a [ServerJar],
) -> InstalledPlugins<'a> {
    InstalledPlugins {
        jars: jars
            .iter()
            .map(|jar| {
                let name = jar
                    .declaration
                    .as_ref()
                    .and_then(|declaration| declaration.name.as_deref());
                (jar.file_name.as_str(), name)
            })
            .collect(),
        record_identifiers: server
            .mism_toml
            .plugins
            .iter()
            .filter_map(|record| plugin_source::identifier_of(&record.source))
            .collect(),
    }
}

/// `declared` always, then each other judgement that applies (insp-3).
fn judgement_lines(judgement: &Judgement) -> Vec<String> {
    let declared = judgement.declared.to_string();
    let mut lines = vec![output::record(&[
        Field::Value(Some("declared")),
        Field::Value(Some(&declared)),
    ])];
    for (name, values) in [
        ("incompatible", &judgement.incompatible),
        ("missing", &judgement.missing),
        ("conflict", &judgement.conflict),
    ] {
        if !values.is_empty() {
            lines.push(output::record(&[
                Field::Value(Some(name)),
                Field::List(values.iter().map(String::as_str).collect()),
            ]));
        }
    }
    lines
}
