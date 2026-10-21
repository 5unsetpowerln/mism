const PREFIX: &str = "modrinth:";
/// The characters of a Modrinth identifier besides ASCII letters and digits.
const IDENTIFIER_PUNCTUATION: &str = "_!@$()`.+,\"-'";
const IDENTIFIER_LENGTHS: std::ops::RangeInclusive<usize> = 3..=64;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "{identifier:?} is not a Modrinth identifier: it must be 3 to 64 characters, each an ASCII \
         letter or digit or one of {IDENTIFIER_PUNCTUATION}"
    )]
    InvalidIdentifier { identifier: String },
    #[error("{text:?} is not `modrinth:` followed by an identifier and nothing else")]
    NotModrinthProject { text: String },
}

impl Error {
    /// None: the remedy for an invalid identifier names the caller's search subcommand.
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::InvalidIdentifier { .. } | Error::NotModrinthProject { .. } => None,
        }
    }
}

/// `modrinth:<identifier>` and the attribute values after it, in the order given (plg-10, dp-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModrinthSource {
    pub identifier: String,
    pub values: Vec<String>,
}

/// The `modrinth:` source `text`, `None` when `text` does not start with `modrinth:` (plg-3,
/// dp-4).
pub fn parse(text: &str) -> Result<Option<ModrinthSource>, Error> {
    let Some(rest) = text.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let mut elements = rest.split(':');
    let identifier = elements
        .next()
        .expect("splitting yields at least one element");
    check_identifier(identifier)?;
    Ok(Some(ModrinthSource {
        identifier: identifier.to_owned(),
        values: elements.map(str::to_owned).collect(),
    }))
}

/// The identifier in `text`, which must be `modrinth:` and an identifier only: the argument of
/// `versions` (vers-4, dvers-4).
pub fn parse_project(text: &str) -> Result<String, Error> {
    let not_modrinth_project = || Error::NotModrinthProject {
        text: text.to_owned(),
    };
    let identifier = text.strip_prefix(PREFIX).ok_or_else(not_modrinth_project)?;
    if identifier.contains(':') {
        return Err(not_modrinth_project());
    }
    check_identifier(identifier)?;
    Ok(identifier.to_owned())
}

/// Checks the identifier against the slug characters of Modrinth's documentation, so that no
/// identifier changes the endpoint it is put in (plg-3, dp-4).
fn check_identifier(identifier: &str) -> Result<(), Error> {
    let valid = IDENTIFIER_LENGTHS.contains(&identifier.chars().count())
        && identifier.chars().all(|character| {
            character.is_ascii_alphanumeric() || IDENTIFIER_PUNCTUATION.contains(character)
        });
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidIdentifier {
            identifier: identifier.to_owned(),
        })
    }
}

/// `modrinth:`, the identifier, and each value after a `:` (plg-12, dp-6).
pub fn source_text(identifier: &str, values: &[&str]) -> String {
    let mut source = format!("{PREFIX}{identifier}");
    for value in values {
        source.push(':');
        source.push_str(value);
    }
    source
}

/// The identifier in a recorded source: the first element after `modrinth:`. Other sources have
/// none.
pub fn identifier_of(source: &str) -> Option<&str> {
    source.strip_prefix(PREFIX)?.split(':').next()
}

/// The version number in a recorded source: the second element after `modrinth:`. Other sources
/// and a `modrinth:` source without it have none.
pub fn version_number_of(source: &str) -> Option<&str> {
    source.strip_prefix(PREFIX)?.split(':').nth(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// plg-3: `modrinth:` gives the identifier and the values in order, empty ones included.
    #[test]
    fn modrinth_source_gives_the_identifier_and_the_values() {
        assert_eq!(
            parse("modrinth:fastasyncworldedit:2.16.1::paper").unwrap(),
            Some(ModrinthSource {
                identifier: "fastasyncworldedit".to_owned(),
                values: vec!["2.16.1".to_owned(), String::new(), "paper".to_owned()],
            })
        );
    }

    /// plg-3: `modrinth:` and an identifier only has no values.
    #[test]
    fn modrinth_source_without_values_has_none() {
        assert_eq!(
            parse("modrinth:luckperms").unwrap(),
            Some(ModrinthSource {
                identifier: "luckperms".to_owned(),
                values: Vec::new(),
            })
        );
    }

    /// plg-3: a text without `modrinth:` is another form.
    #[test]
    fn text_without_the_prefix_is_not_a_modrinth_source() {
        assert_eq!(parse("https://example.com/a.jar").unwrap(), None);
    }

    /// d-source-3
    #[test]
    fn identifier_with_a_slash_is_rejected() {
        assert!(matches!(
            parse("modrinth:../search"),
            Err(Error::InvalidIdentifier { .. })
        ));
    }

    /// plg-3: identifiers have 3 to 64 characters.
    #[test]
    fn identifiers_of_3_and_64_characters_are_accepted() {
        let rejected: Vec<_> = ["abc".to_owned(), "a".repeat(64)]
            .into_iter()
            .filter(|identifier| check_identifier(identifier).is_err())
            .collect();
        assert!(rejected.is_empty(), "{rejected:?}");
    }

    /// plg-3: identifiers of 2 and 65 characters are rejected.
    #[test]
    fn identifiers_of_2_and_65_characters_are_rejected() {
        let accepted: Vec<_> = ["ab".to_owned(), "a".repeat(65)]
            .into_iter()
            .filter(|identifier| check_identifier(identifier).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// plg-3: every character of the documented set is accepted.
    #[test]
    fn documented_punctuation_is_accepted() {
        assert!(check_identifier("Az09_!@$()`.+,\"-'").is_ok());
    }

    /// plg-3: characters outside the set (`/`, `:`, space, non-ASCII) are rejected.
    #[test]
    fn other_characters_are_rejected() {
        let accepted: Vec<_> = ["a/bc", "a bc", "abcé", "ab#c", "ab%c"]
            .into_iter()
            .filter(|identifier| check_identifier(identifier).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// vers-4: `modrinth:` and an identifier is the argument of `versions`.
    #[test]
    fn project_argument_gives_the_identifier() {
        assert_eq!(parse_project("modrinth:luckperms").unwrap(), "luckperms");
    }

    /// vers-4: without `modrinth:`, with a value after the identifier, or with an invalid
    /// identifier, the argument is rejected.
    #[test]
    fn other_project_arguments_are_rejected() {
        let accepted: Vec<_> = [
            "luckperms",
            "modrinth:luckperms:v5.5.71-bukkit",
            "modrinth:luckperms:",
            "modrinth:lp",
            "https://example.com/a.jar",
        ]
        .into_iter()
        .filter(|text| parse_project(text).is_ok())
        .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// plg-12
    #[test]
    fn source_text_joins_the_values_with_colons() {
        assert_eq!(
            source_text("fastasyncworldedit", &["2.16.1", "paper"]),
            "modrinth:fastasyncworldedit:2.16.1:paper"
        );
    }

    /// 「取得元の指定」 方法: the identifier of a recorded Modrinth source.
    #[test]
    fn identifier_of_a_modrinth_source_is_its_first_element() {
        assert_eq!(
            identifier_of("modrinth:luckperms:v5.5.71-bukkit"),
            Some("luckperms")
        );
    }

    /// 「取得元の指定」 方法: the version number of a recorded Modrinth source.
    #[test]
    fn version_number_of_a_modrinth_source_is_its_second_element() {
        assert_eq!(
            version_number_of("modrinth:fastasyncworldedit:2.16.1:paper"),
            Some("2.16.1")
        );
    }

    /// 「取得元の指定」 方法: URL and local file sources have no identifier or version number.
    #[test]
    fn other_sources_have_no_version_number() {
        let found: Vec<_> = [
            "https://example.com/a.jar",
            "plugins/a.jar",
            "modrinth:luckperms",
        ]
        .into_iter()
        .filter_map(version_number_of)
        .collect();
        assert!(found.is_empty(), "{found:?}");
    }
}
