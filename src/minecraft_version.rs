use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unrecognized Minecraft version `{text}`")]
    Unrecognized { text: String },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        None
    }
}

/// Variant order is the release order: snapshot < pre-release < release candidate < release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Suffix {
    Snapshot(u64),
    PreRelease(u64),
    ReleaseCandidate(u64),
    Release,
}

#[derive(Debug, Clone)]
pub struct MinecraftVersion {
    text: String,
    // Trailing zeros removed, so lexicographic order equals zero-padded element-wise order.
    numbers: Vec<u64>,
    suffix: Suffix,
}

impl MinecraftVersion {
    pub fn parse(text: &str) -> Result<MinecraftVersion, Error> {
        let unrecognized = || Error::Unrecognized {
            text: text.to_owned(),
        };
        let (numbers_text, suffix_text) = match text.split_once('-') {
            Some((numbers_text, suffix_text)) => (numbers_text, Some(suffix_text)),
            None => (text, None),
        };
        let mut numbers = parse_numbers(numbers_text).ok_or_else(unrecognized)?;
        while numbers.last() == Some(&0) {
            numbers.pop();
        }
        let suffix = match suffix_text {
            None => Suffix::Release,
            Some(suffix_text) => parse_suffix(suffix_text).ok_or_else(unrecognized)?,
        };
        Ok(MinecraftVersion {
            text: text.to_owned(),
            numbers,
            suffix,
        })
    }
}

fn parse_numbers(text: &str) -> Option<Vec<u64>> {
    text.split('.').map(parse_number).collect()
}

/// Accepts ASCII digits only; `u64::from_str` would also accept a leading `+`.
fn parse_number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn parse_suffix(text: &str) -> Option<Suffix> {
    if let Some(rest) = text.strip_prefix("snapshot") {
        return parse_suffix_number(rest).map(Suffix::Snapshot);
    }
    if let Some(rest) = text.strip_prefix("pre") {
        return parse_suffix_number(rest).map(Suffix::PreRelease);
    }
    let rest = text.strip_prefix("rc")?;
    parse_suffix_number(rest).map(Suffix::ReleaseCandidate)
}

fn parse_suffix_number(text: &str) -> Option<u64> {
    parse_number(text.strip_prefix('-').unwrap_or(text))
}

impl PartialEq for MinecraftVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for MinecraftVersion {}

impl PartialOrd for MinecraftVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MinecraftVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers
            .cmp(&other.numbers)
            .then(self.suffix.cmp(&other.suffix))
    }
}

impl fmt::Display for MinecraftVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

/// The newest of `versions`, leaving out the ones mism cannot parse; the first of equally new
/// ones. None when mism can parse none of them (srch-2, vers-2, dsrch-2, dvers-2).
pub fn newest(versions: &[String]) -> Option<&str> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> MinecraftVersion {
        MinecraftVersion::parse(text).unwrap()
    }

    /// d-ver-1
    #[test]
    fn longer_number_list_is_newer() {
        assert!(version("26.1.2") > version("26.1"));
    }

    /// d-ver-2
    #[test]
    fn release_is_newer_than_its_release_candidate() {
        assert!(version("26.3") > version("26.3-rc-3"));
    }

    /// d-ver-3
    #[test]
    fn larger_pre_release_number_is_newer() {
        assert!(version("1.21.11-pre6") > version("1.21.11-pre5"));
    }

    /// d-ver-4
    #[test]
    fn listed_versions_sort_oldest_first() {
        let expected = ["1.21.11-rc3", "1.21.11", "26.1.2", "26.3-rc-3", "26.3"];
        let mut versions: Vec<_> = ["26.3", "1.21.11", "26.3-rc-3", "26.1.2", "1.21.11-rc3"]
            .into_iter()
            .map(version)
            .collect();
        versions.sort();
        let sorted: Vec<_> = versions.iter().map(ToString::to_string).collect();
        assert_eq!(sorted, expected);
    }

    /// d-ver-5
    #[test]
    fn unrecognized_text_error_contains_the_text() {
        let error = MinecraftVersion::parse("26.3-snapshot").unwrap_err();
        assert!(error.to_string().contains("26.3-snapshot"));
    }

    /// d-ver-5
    #[test]
    fn malformed_texts_are_rejected() {
        let rejected: Vec<_> = [
            "",
            "26.",
            ".26",
            "26..3",
            "26.3-pre",
            "26.3-snapshot-",
            "26.3-rc-",
            "+1",
            "26.3-pre--1",
            "26.3-",
        ]
        .into_iter()
        .filter(|text| MinecraftVersion::parse(text).is_ok())
        .collect();
        assert!(rejected.is_empty(), "{rejected:?}");
    }

    /// d-ver-5
    #[test]
    fn all_suffix_spellings_parse() {
        let rejected: Vec<_> = [
            "1.21.11-pre5",
            "26.1-pre-1",
            "1.21.11-rc3",
            "26.3-rc-3",
            "26.4-snapshot-5",
            "26.4-snapshot5",
        ]
        .into_iter()
        .filter(|text| MinecraftVersion::parse(text).is_err())
        .collect();
        assert!(rejected.is_empty(), "{rejected:?}");
    }

    /// d-ver-6
    #[test]
    fn snapshot_pre_release_release_candidate_and_release_sort_oldest_first() {
        let expected = ["26.3-snapshot-10", "26.3-pre-1", "26.3-rc-1", "26.3"];
        let mut versions: Vec<_> = ["26.3", "26.3-rc-1", "26.3-snapshot-10", "26.3-pre-1"]
            .into_iter()
            .map(version)
            .collect();
        versions.sort();
        let sorted: Vec<_> = versions.iter().map(ToString::to_string).collect();
        assert_eq!(sorted, expected);
    }

    /// d-ver-7
    #[test]
    fn larger_snapshot_number_is_newer() {
        assert!(version("26.3-snapshot-10") > version("26.3-snapshot-9"));
    }

    /// Ordering section: both spellings of the same suffix are equal.
    #[test]
    fn dashed_and_undashed_suffixes_are_equal() {
        assert_eq!(version("26.3-rc3"), version("26.3-rc-3"));
    }

    /// Ordering section: trailing zeros are padding.
    #[test]
    fn trailing_zero_number_is_equal() {
        assert_eq!(version("26.3"), version("26.3.0"));
    }

    /// Ordering section: pre-release is older than release candidate.
    #[test]
    fn pre_release_is_older_than_release_candidate() {
        assert!(version("26.3-pre9") < version("26.3-rc1"));
    }

    /// Display and as_str keep the original text.
    #[test]
    fn display_prints_original_text() {
        assert_eq!(version("26.3.0").to_string(), "26.3.0");
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    /// d-compat-5
    #[test]
    fn newest_minecraft_version_includes_snapshots_and_skips_unparsable_ones() {
        assert_eq!(
            newest(&strings(&["26.3", "26.4-snapshot-5", "25w46a"])),
            Some("26.4-snapshot-5")
        );
    }

    /// d-compat-6
    #[test]
    fn no_parsable_minecraft_version_has_no_newest() {
        assert_eq!(newest(&strings(&["25w46a", "b1.7.3"])), None);
    }

    /// 「Minecraft のバージョンの新旧の判定」 方法: of equally new versions the first is the newest.
    #[test]
    fn first_of_equally_new_versions_is_the_newest() {
        assert_eq!(newest(&strings(&["1.21", "1.21.0"])), Some("1.21"));
    }
}
