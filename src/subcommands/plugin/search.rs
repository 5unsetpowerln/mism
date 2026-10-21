use std::ops::RangeInclusive;

use crate::command_line::SearchArguments;
use crate::compatibility;
use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::modrinth_api::response::{SearchHit, Version};
use crate::modrinth_api::{self, HttpModrinthApi, ModrinthApi};
use crate::output::{self, Field};
use crate::plugin_source;

const DEFAULT_LIMIT: u32 = 10;
const LIMITS: RangeInclusive<u32> = 10..=100;
/// The server software mism manages, as Modrinth names them in `loaders` (srch-2).
const SERVER_SOFTWARE: [&str; 2] = ["paper", "folia"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "--limit {text:?} is not an integer from {} to {}",
        LIMITS.start(),
        LIMITS.end()
    )]
    InvalidLimit { text: String },
    #[error(transparent)]
    Http(#[from] http_client::Error),
    #[error(transparent)]
    Modrinth(#[from] modrinth_api::Error),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Http(error) => error.hint(),
            Error::Modrinth(error) => error.hint(),
            Error::Output(error) => error.hint(),
            Error::InvalidLimit { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Http(error) => error.interrupted_by(),
            Error::Modrinth(error) => error.interrupted_by(),
            Error::InvalidLimit { .. } | Error::Output(_) => None,
        }
    }
}

/// `mism plugin search` (srch-1 to srch-5). It writes the plugins only after every request
/// succeeded (srch-4).
pub fn run(arguments: &SearchArguments) -> Result<(), Error> {
    let limit = limit_of(arguments.limit.as_deref())?;
    let api = HttpModrinthApi::new(HttpClient::new()?)?;
    let hits = api.search(&arguments.term, limit)?;
    let newest = newest_server_versions(&api, &hits)?;
    write_found(&hits, &newest)
}

/// Step 1: 10 without `--limit`, else its value, which is ASCII digits only and from 10 to 100
/// (srch-1, srch-5).
fn limit_of(text: Option<&str>) -> Result<u32, Error> {
    let Some(text) = text else {
        return Ok(DEFAULT_LIMIT);
    };
    let invalid = || Error::InvalidLimit {
        text: text.to_owned(),
    };
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    // Digits only, so parsing fails only on the empty text and on numbers too large for `u32`.
    text.parse()
        .ok()
        .filter(|limit| LIMITS.contains(limit))
        .ok_or_else(invalid)
}

/// Steps 3 and 4: for each plugin found, in Modrinth's order, its newest version for Paper or
/// Folia (srch-2).
fn newest_server_versions(
    api: &dyn ModrinthApi,
    hits: &[SearchHit],
) -> Result<Vec<Option<Version>>, Error> {
    hits.iter()
        .map(|hit| {
            Ok(newest_server_version(
                api.project_versions(&hit.project_id)?,
            ))
        })
        .collect()
}

/// Step 4: of the versions for Paper or Folia, the newest; of equally new ones, the first in the
/// list (srch-2).
fn newest_server_version(versions: Vec<Version>) -> Option<Version> {
    versions
        .into_iter()
        .filter(|version| {
            version
                .loaders
                .iter()
                .any(|loader| SERVER_SOFTWARE.contains(&loader.as_str()))
        })
        .reduce(|newest, version| {
            if version.date_published > newest.date_published {
                version
            } else {
                newest
            }
        })
}

/// Step 5: one record per plugin found, in Modrinth's order (srch-1, srch-2, srch-3, plg-9).
fn write_found(hits: &[SearchHit], newest: &[Option<Version>]) -> Result<(), Error> {
    let lines: Vec<String> = hits
        .iter()
        .zip(newest)
        .map(|(hit, version)| found_line(hit, version.as_ref()))
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

/// The identifier, the plugin name, and the version number, the server software, and the newest
/// Minecraft version of `version`, each `-` without it (srch-2, plg-9).
fn found_line(hit: &SearchHit, version: Option<&Version>) -> String {
    let source = plugin_source::modrinth_source(&hit.slug, &[]);
    output::record(&[
        Field::Value(Some(&source)),
        Field::Value(Some(&hit.title)),
        Field::Value(version.map(|version| version.version_number.as_str())),
        Field::List(version.map_or_else(Vec::new, |version| {
            version.loaders.iter().map(String::as_str).collect()
        })),
        Field::Value(
            version.and_then(|version| {
                compatibility::newest_minecraft_version(&version.game_versions)
            }),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// srch-5: a limit of leading zeros is still an integer of digits only.
    #[test]
    fn limit_with_leading_zeros_is_its_integer() {
        assert_eq!(limit_of(Some("010")).unwrap(), 10);
    }

    /// srch-5: a limit too large for any integer type mism uses is an error.
    #[test]
    fn limit_beyond_the_integer_range_is_an_error() {
        assert!(matches!(
            limit_of(Some("99999999999999999999")),
            Err(Error::InvalidLimit { .. })
        ));
    }

    /// srch-5: an empty limit is an error.
    #[test]
    fn empty_limit_is_an_error() {
        assert!(matches!(
            limit_of(Some("")),
            Err(Error::InvalidLimit { .. })
        ));
    }
}
