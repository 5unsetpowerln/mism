use super::{CacheError, SourceError, fetch};
use crate::command_line::DatapackInspectArguments;
use crate::datapack_compatibility::{self, Agreement, Inspection, Judgement, Target};
use crate::datapack_zip::{self, FormatRange};
use crate::fetched_file_cache::CachedFile;
use crate::interruption::Signal;
use crate::modrinth_source;
use crate::output::{self, Field};
use crate::server_directory::ServerDirectory;
use crate::server_directory::server_jar;
use crate::subcommands::target_server;

/// How dinsp-2 writes a value that nothing tells.
const UNKNOWN: &str = "unknown";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TargetServer(#[from] target_server::Error),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error(transparent)]
    Fetch(#[from] fetch::Error),
    #[error(transparent)]
    Declaration(#[from] datapack_zip::Error),
    #[error(transparent)]
    ServerJar(#[from] server_jar::Error),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            // The cached datapack file could not be read.
            Error::Declaration(
                datapack_zip::Error::ReadFile { path, .. }
                | datapack_zip::Error::ReadPackMcmeta { path, .. },
            ) => Some(super::refetch_hint(path)),
            Error::TargetServer(error) => error.hint(),
            Error::Source(error) => error.hint(),
            Error::Cache(error) => error.hint(),
            Error::Fetch(error) => error.hint(),
            Error::Declaration(error) => error.hint(),
            Error::ServerJar(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Cache(error) => error.interrupted_by(),
            Error::Fetch(error) => error.interrupted_by(),
            Error::TargetServer(_)
            | Error::Source(_)
            | Error::Declaration(_)
            | Error::ServerJar(_)
            | Error::Output(_) => None,
        }
    }
}

/// `mism datapack inspect` (dinsp-1).
pub fn run(arguments: &DatapackInspectArguments) -> Result<(), Error> {
    let server = target_server::current()?;
    let source = super::parse_source(&arguments.source)?;
    let cached_file = match super::find_in_cache(&arguments.source, &source)? {
        Some(cached_file) => cached_file,
        None => fetch::fetch_file(&arguments.source, &source, arguments.yes, None)?,
    };
    inspect_file(&cached_file, server.as_ref())
}

/// Steps 4 to 6, the inspection procedure (検証の手順) that `place` and `install` use too: the
/// inspection, and with a target server the judgements, after the declaration and the server's
/// format number were read (dinsp-2 to dinsp-5). The judgements stop nothing (dplace-6).
pub fn inspect_file(
    cached_file: &CachedFile,
    server: Option<&ServerDirectory>,
) -> Result<(), Error> {
    let declaration = datapack_zip::read_declaration(&cached_file.path)?;
    let target = server
        .map(|server| read_target(server).map(|target| (server, target)))
        .transpose()?;
    let inspection = datapack_compatibility::inspect(cached_file, declaration);
    output::write_to_stdout(&inspection_lines(&inspection).join("\n"))?;
    if let Some((server, target)) = target {
        write_judgement(&inspection, &target, server)?;
    }
    Ok(())
}

/// Step 4 with a target server: its Minecraft version and the data pack format number of its
/// server jar (dinsp-5).
fn read_target(server: &ServerDirectory) -> Result<Target<'_>, Error> {
    let mism_toml = &server.mism_toml;
    let format = server_jar::read_data_pack_format(
        &server.path,
        &mism_toml.software,
        &mism_toml.minecraft_version,
    )?;
    Ok(Target {
        minecraft_version: &mism_toml.minecraft_version,
        format_major: format.major,
        format_minor: format.minor,
    })
}

/// The name and the value of each of the 9 values (dinsp-2).
fn inspection_lines(inspection: &Inspection) -> Vec<String> {
    let file = &inspection.file;
    let (min_format, max_format) = format_bounds(inspection.declaration.as_ref());
    let modrinth = file.modrinth.as_ref();
    vec![
        value_line("name", Some(&file.name)),
        value_line("version", file.version_number()),
        value_line("source", Some(&file.source)),
        value_line("file.name", Some(&file.file_name)),
        value_line("file.sha256", Some(&file.sha256)),
        known_line(
            "declaration.min_format",
            min_format.as_deref().map(|min| Field::Value(Some(min))),
        ),
        known_line(
            "declaration.max_format",
            max_format.as_deref().map(|max| Field::Value(Some(max))),
        ),
        known_line(
            "modrinth.minecraft_versions",
            modrinth.map(|modrinth| {
                Field::MinecraftVersions(
                    modrinth.game_versions.iter().map(String::as_str).collect(),
                )
            }),
        ),
        known_line(
            "modrinth.required_dependencies",
            modrinth.map(|modrinth| {
                Field::List(
                    modrinth
                        .required_dependencies
                        .iter()
                        .map(String::as_str)
                        .collect(),
                )
            }),
        ),
    ]
}

fn value_line(name: &str, value: Option<&str>) -> String {
    output::record(&[Field::Value(Some(name)), Field::Value(value)])
}

/// A value that is `unknown` when it is `None`.
fn known_line(name: &str, value: Option<Field>) -> String {
    output::record(&[
        Field::Value(Some(name)),
        value.unwrap_or(Field::Value(Some(UNKNOWN))),
    ])
}

/// The lower and the upper bound as dinsp-2 writes them, none when the declaration tells none.
fn format_bounds(range: Option<&FormatRange>) -> (Option<String>, Option<String>) {
    match range {
        Some(range) => (Some(range.min.to_string()), Some(range.max.to_string())),
        None => (None, None),
    }
}

/// Step 6: the judgements of dinsp-3 on the target server, whose plugin and datapack records
/// both count for `missing`.
fn write_judgement(
    inspection: &Inspection,
    target: &Target,
    server: &ServerDirectory,
) -> Result<(), Error> {
    let mism_toml = &server.mism_toml;
    let record_identifiers: Vec<&str> = mism_toml
        .plugins
        .iter()
        .chain(&mism_toml.datapacks)
        .filter_map(|record| modrinth_source::identifier_of(&record.source))
        .collect();
    let judgement = datapack_compatibility::judge(inspection, target, &record_identifiers);
    output::write_to_stdout(&judgement_lines(&judgement).join("\n"))?;
    write_mismatch_warnings(&judgement, inspection, target)
}

/// After the judgements, one warning for each of `declared` and `pack_format` that is
/// `mismatch`, in that order (dinsp-6).
fn write_mismatch_warnings(
    judgement: &Judgement,
    inspection: &Inspection,
    target: &Target,
) -> Result<(), Error> {
    if judgement.declared == Agreement::Mismatch {
        output::write_warning(&format!(
            "Modrinth does not list Minecraft {} for this version of the datapack",
            target.minecraft_version
        ))?;
    }
    // `pack_format` is `mismatch` only for declared formats.
    if let (Agreement::Mismatch, Some(range)) = (judgement.pack_format, &inspection.declaration) {
        output::write_warning(&format!(
            "the datapack declares the data pack formats {} to {}, which do not include {}.{} \
             of the server",
            range.min, range.max, target.format_major, target.format_minor
        ))?;
    }
    Ok(())
}

/// `declared` and `pack_format` always, then `missing` when it applies (dinsp-3).
fn judgement_lines(judgement: &Judgement) -> Vec<String> {
    let mut lines = vec![
        agreement_line("declared", judgement.declared),
        agreement_line("pack_format", judgement.pack_format),
    ];
    if !judgement.missing.is_empty() {
        lines.push(output::record(&[
            Field::Value(Some("missing")),
            Field::List(judgement.missing.iter().map(String::as_str).collect()),
        ]));
    }
    lines
}

fn agreement_line(name: &str, agreement: Agreement) -> String {
    let agreement = agreement.to_string();
    output::record(&[Field::Value(Some(name)), Field::Value(Some(&agreement))])
}
