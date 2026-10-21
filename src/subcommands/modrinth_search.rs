use std::ops::RangeInclusive;

use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::modrinth_api::response::{SearchHit, Version};
use crate::modrinth_api::{self, HttpModrinthApi, ModrinthApi};

const DEFAULT_LIMIT: u32 = 10;
const LIMITS: RangeInclusive<u32> = 10..=100;

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
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Http(error) => error.hint(),
            Error::Modrinth(error) => error.hint(),
            Error::InvalidLimit { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Http(error) => error.interrupted_by(),
            Error::Modrinth(error) => error.interrupted_by(),
            Error::InvalidLimit { .. } => None,
        }
    }
}

/// A project found, and its newest version that describes it; none when no version does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub hit: SearchHit,
    pub version: Option<Version>,
}

/// Steps 1 to 4 of the search: the projects found for `term` with the filter `facets`, as many as
/// `limit` (the value of `--limit`) allows, in Modrinth's order, each with its newest version that
/// `describes` it (srch-1, srch-2, srch-5, dsrch-1, dsrch-2, dsrch-5). Every request has
/// succeeded when it returns (srch-4, dsrch-4).
pub fn search(
    term: &str,
    limit: Option<&str>,
    facets: &str,
    describes: fn(&Version) -> bool,
) -> Result<Vec<Found>, Error> {
    let limit = limit_of(limit)?;
    let api = HttpModrinthApi::new(HttpClient::new()?)?;
    let hits = api.search(term, facets, limit)?;
    hits.into_iter()
        .map(|hit| {
            let version =
                newest_described_version(api.project_versions(&hit.project_id)?, describes);
            Ok(Found { hit, version })
        })
        .collect()
}

/// Step 1: 10 without `--limit`, else its value, which is ASCII digits only and from 10 to 100
/// (srch-1, srch-5, dsrch-1, dsrch-5).
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

/// Step 4: of the versions that describe the project, the newest; of equally new ones, the first
/// in the list (srch-2, dsrch-2).
fn newest_described_version(
    versions: Vec<Version>,
    describes: fn(&Version) -> bool,
) -> Option<Version> {
    versions
        .into_iter()
        .filter(describes)
        .reduce(|newest, version| {
            if version.date_published > newest.date_published {
                version
            } else {
                newest
            }
        })
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
