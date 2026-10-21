use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

const FILE_NAME: &str = "server.properties";
const LEVEL_NAME: &str = "level-name";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{} does not exist", path.display())]
    Missing { path: PathBuf },
    #[error("failed to read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not UTF-8", path.display())]
    NotUtf8 {
        path: PathBuf,
        #[source]
        source: FromUtf8Error,
    },
    #[error("{} has an escape that cannot be read", path.display())]
    InvalidEscape {
        path: PathBuf,
        #[source]
        source: EscapeError,
    },
    #[error("{} has no {LEVEL_NAME}", path.display())]
    NoLevelName { path: PathBuf },
    #[error("the {LEVEL_NAME} in {} is empty", path.display())]
    EmptyLevelName { path: PathBuf },
}

/// An escape that rule 4 of 「server.properties の読み取り」 cannot undo.
#[derive(Debug, thiserror::Error)]
pub enum EscapeError {
    #[error("{escape:?} is not `\\u` followed by 4 hexadecimal digits")]
    NotHexadecimal { escape: String },
    #[error("the `\\u` escapes of {text:?} are not valid UTF-16")]
    NotUtf16 { text: String },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Missing { .. } => Some(format!(
                "start the server once so that it creates {FILE_NAME}"
            )),
            Error::NoLevelName { path } | Error::EmptyLevelName { path } => Some(format!(
                "write the path of the world directory as {LEVEL_NAME} in {}",
                path.display()
            )),
            Error::Read { .. } | Error::NotUtf8 { .. } | Error::InvalidEscape { .. } => None,
        }
    }
}

/// The whitespace of rules 1 to 3: space, tab, and form feed.
const WHITESPACE: [char; 3] = [' ', '\t', '\x0c'];

/// The `level-name` of the `server.properties` directly in `server_directory`, which must be there
/// and must not be empty (dp-3).
pub fn read_level_name(server_directory: &Path) -> Result<String, Error> {
    let path = server_directory.join(FILE_NAME);
    let text = read_text(&path)?;
    level_name(&text, &path)
}

fn read_text(path: &Path) -> Result<String, Error> {
    let bytes = fs::read(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            Error::Missing {
                path: path.to_path_buf(),
            }
        } else {
            Error::Read {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    String::from_utf8(bytes).map_err(|source| Error::NotUtf8 {
        path: path.to_path_buf(),
        source,
    })
}

fn level_name(text: &str, path: &Path) -> Result<String, Error> {
    let mut properties = properties(text).map_err(|source| Error::InvalidEscape {
        path: path.to_path_buf(),
        source,
    })?;
    match properties.remove(LEVEL_NAME) {
        None => Err(Error::NoLevelName {
            path: path.to_path_buf(),
        }),
        Some(value) if value.is_empty() => Err(Error::EmptyLevelName {
            path: path.to_path_buf(),
        }),
        Some(value) => Ok(value),
    }
}

/// Every key and its value, read as `java.util.Properties` reads its format; of a key written
/// more than once, the last value (rule 5).
fn properties(text: &str) -> Result<HashMap<String, String>, EscapeError> {
    let mut properties = HashMap::new();
    for line in logical_lines(text) {
        let (key, value) = split_key_and_value(&line);
        properties.insert(unescape(key)?, unescape(value)?);
    }
    Ok(properties)
}

/// Rules 1 and 2: the lines to read, each continued line joined to the lines after it.
fn logical_lines(text: &str) -> Vec<String> {
    let mut parts = natural_lines(text).into_iter();
    let mut lines = Vec::new();
    while let Some(part) = parts.next() {
        let part = without_leading_whitespace(part);
        if part.is_empty() || part.starts_with(['#', '!']) {
            continue;
        }
        let mut line = part.to_owned();
        while ends_with_odd_backslashes(&line) {
            line.pop();
            let Some(next) = parts.next() else {
                break;
            };
            line.push_str(without_leading_whitespace(next));
        }
        lines.push(line);
    }
    lines
}

/// The parts of `text` between `\n`, `\r\n`, and `\r`.
fn natural_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut rest = text;
    while let Some(end) = rest.find(['\n', '\r']) {
        lines.push(&rest[..end]);
        let newline = if rest[end..].starts_with("\r\n") {
            2
        } else {
            1
        };
        rest = &rest[end + newline..];
    }
    lines.push(rest);
    lines
}

fn without_leading_whitespace(text: &str) -> &str {
    text.trim_start_matches(WHITESPACE)
}

fn ends_with_odd_backslashes(line: &str) -> bool {
    line.chars()
        .rev()
        .take_while(|&character| character == '\\')
        .count()
        % 2
        == 1
}

/// Rule 3: the key up to the first unescaped `=`, `:`, or whitespace, and the value after it.
fn split_key_and_value(line: &str) -> (&str, &str) {
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if !escaped && matches!(character, '=' | ':') {
            return (
                &line[..index],
                without_leading_whitespace(&line[index + 1..]),
            );
        }
        if !escaped && WHITESPACE.contains(&character) {
            let rest = without_leading_whitespace(&line[index + 1..]);
            let value = rest
                .strip_prefix(['=', ':'])
                .map_or(rest, without_leading_whitespace);
            return (&line[..index], value);
        }
        escaped = character == '\\' && !escaped;
    }
    (line, "")
}

/// Rule 4: the text with its escapes undone. `\u` escapes are UTF-16 code units, so the text is
/// built as UTF-16.
fn unescape(text: &str) -> Result<String, EscapeError> {
    let mut units: Vec<u16> = Vec::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            units.extend(character.encode_utf16(&mut [0; 2]).iter());
            continue;
        }
        let escaped = characters
            .next()
            .expect("rules 2 and 3 leave no backslash that escapes nothing");
        let unescaped = match escaped {
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            'f' => '\x0c',
            'u' => {
                units.push(code_unit(&mut characters)?);
                continue;
            }
            other => other,
        };
        units.extend(unescaped.encode_utf16(&mut [0; 2]).iter());
    }
    String::from_utf16(&units).map_err(|_| EscapeError::NotUtf16 {
        text: text.to_owned(),
    })
}

/// The code unit of the 4 hexadecimal digits after `\u`.
fn code_unit(characters: &mut std::str::Chars) -> Result<u16, EscapeError> {
    let digits: String = characters.by_ref().take(4).collect();
    let is_hexadecimal =
        digits.chars().count() == 4 && digits.chars().all(|digit| digit.is_ascii_hexdigit());
    if !is_hexadecimal {
        return Err(EscapeError::NotHexadecimal {
            escape: format!("\\u{digits}"),
        });
    }
    Ok(u16::from_str_radix(&digits, 16).expect("4 hexadecimal digits fit in 16 bits"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn level_name_of(text: &str) -> Result<String, Error> {
        level_name(text, Path::new("/server/server.properties"))
    }

    fn value_of(text: &str, key: &str) -> Option<String> {
        properties(text).unwrap().remove(key)
    }

    /// d-props-1
    #[test]
    fn escaped_colon_is_a_colon() {
        assert_eq!(
            level_name_of("level-name=my\\:world\n").unwrap(),
            "my:world"
        );
    }

    /// d-props-2
    #[test]
    fn spaces_around_the_equals_sign_are_not_in_the_value() {
        assert_eq!(level_name_of("level-name = world\n").unwrap(), "world");
    }

    /// d-props-3
    #[test]
    fn comment_line_is_not_read() {
        assert_eq!(
            level_name_of("#level-name=other\nlevel-name=world\n").unwrap(),
            "world"
        );
    }

    /// d-props-4
    #[test]
    fn backslash_at_the_end_continues_on_the_next_line_without_its_leading_spaces() {
        assert_eq!(
            level_name_of("level-name=my\\\n  survival\n").unwrap(),
            "mysurvival"
        );
    }

    /// d-props-5
    #[test]
    fn unicode_escape_is_its_character() {
        assert_eq!(level_name_of("level-name=caf\\u00e9\n").unwrap(), "café");
    }

    /// 「server.properties の読み取り」 規則 1: `\r\n` and `\r` end lines too.
    #[test]
    fn carriage_returns_end_lines() {
        assert_eq!(
            (
                value_of("a=1\r\nb=2\rc=3", "b"),
                value_of("a=1\r\nb=2\rc=3", "c")
            ),
            (Some("2".to_owned()), Some("3".to_owned()))
        );
    }

    /// 「server.properties の読み取り」 規則 1: `!` begins a comment too, also after whitespace.
    #[test]
    fn exclamation_mark_after_whitespace_begins_a_comment() {
        assert_eq!(
            properties(" \t!level-name=other\n\x0c# a=b\n").unwrap(),
            HashMap::new()
        );
    }

    /// 「server.properties の読み取り」 規則 1: blank lines are not read.
    #[test]
    fn blank_lines_are_not_read() {
        assert_eq!(properties("\n   \n\t\n").unwrap(), HashMap::new());
    }

    /// 「server.properties の読み取り」 規則 2: an even number of backslashes at the end does not
    /// continue the line.
    #[test]
    fn even_backslashes_at_the_end_do_not_continue_the_line() {
        assert_eq!(
            (
                value_of("a=b\\\\\nc=d\n", "a"),
                value_of("a=b\\\\\nc=d\n", "c")
            ),
            (Some("b\\".to_owned()), Some("d".to_owned()))
        );
    }

    /// 「server.properties の読み取り」 規則 2: a continued line can continue again, and a
    /// continuation that looks like a comment is part of the value.
    #[test]
    fn continuation_continues_again_and_is_never_a_comment() {
        assert_eq!(
            value_of("a=one\\\n  #two\\\n\tthree\n", "a"),
            Some("one#twothree".to_owned())
        );
    }

    /// 「server.properties の読み取り」 規則 2: a backslash at the end of the text is removed.
    #[test]
    fn backslash_at_the_end_of_the_text_is_removed() {
        assert_eq!(value_of("a=b\\", "a"), Some("b".to_owned()));
    }

    /// 「server.properties の読み取り」 規則 3: whitespace separates the key from the value, and an
    /// `=` or `:` after it is skipped with the whitespace that follows.
    #[test]
    fn whitespace_then_separator_separates_the_key() {
        let read: Vec<_> = ["a b", "a  : b", "a\t=\tb"]
            .into_iter()
            .map(|text| value_of(text, "a"))
            .collect();
        assert_eq!(
            read,
            [
                Some("b".to_owned()),
                Some("b".to_owned()),
                Some("b".to_owned())
            ]
        );
    }

    /// 「server.properties の読み取り」 規則 3: only one separator is skipped after whitespace.
    #[test]
    fn second_separator_is_part_of_the_value() {
        assert_eq!(value_of("a = =b", "a"), Some("=b".to_owned()));
    }

    /// 「server.properties の読み取り」 規則 3: an escaped separator is part of the key, and the
    /// value keeps its inner and trailing spaces.
    #[test]
    fn escaped_separator_is_part_of_the_key() {
        assert_eq!(
            value_of("a\\=b\\ c=my world \n", "a=b c"),
            Some("my world ".to_owned())
        );
    }

    /// 「server.properties の読み取り」 規則 3: a key without a separator has an empty value.
    #[test]
    fn key_without_a_separator_has_an_empty_value() {
        assert_eq!(value_of("level-name\n", "level-name"), Some(String::new()));
    }

    /// 「server.properties の読み取り」 規則 4: the escapes of tab, newline, carriage return, and form
    /// feed, and any other escaped character as itself.
    #[test]
    fn escapes_become_their_characters() {
        assert_eq!(
            value_of("a=\\t\\n\\r\\f\\q\\\\", "a"),
            Some("\t\n\r\x0cq\\".to_owned())
        );
    }

    /// 「server.properties の読み取り」 規則 4: two `\u` escapes make one character of a surrogate
    /// pair.
    #[test]
    fn surrogate_pair_of_escapes_is_one_character() {
        assert_eq!(value_of("a=\\ud83d\\uDE00", "a"), Some("😀".to_owned()));
    }

    /// 「server.properties の読み取り」 規則 4: `\u` without 4 hexadecimal digits is an error.
    #[test]
    fn unicode_escape_without_four_hexadecimal_digits_is_an_error() {
        let accepted: Vec<_> = ["a=\\u00g1", "a=\\u00e", "a\\u12=b"]
            .into_iter()
            .filter(|text| properties(text).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// 「server.properties の読み取り」 規則 4: a lone surrogate is an error.
    #[test]
    fn lone_surrogate_is_an_error() {
        assert!(matches!(
            properties("a=\\ud83d"),
            Err(EscapeError::NotUtf16 { .. })
        ));
    }

    /// 「server.properties の読み取り」 規則 5: the last of the same key is used.
    #[test]
    fn last_value_of_a_key_is_used() {
        assert_eq!(
            level_name_of("level-name=first\nlevel-name=second\n").unwrap(),
            "second"
        );
    }

    /// dp-3: a `server.properties` that Paper wrote gives its `level-name`.
    #[test]
    fn server_properties_written_by_paper_gives_its_level_name() {
        let directory = temporary_directory();
        fs::write(
            directory.path().join("server.properties"),
            "#Minecraft server properties\n#Wed Oct 21 10:00:00 JST 2026\n\
             level-type=minecraft\\:normal\nlevel-name=world\nmotd=A Minecraft Server\n",
        )
        .unwrap();

        assert_eq!(read_level_name(directory.path()).unwrap(), "world");
    }

    /// dp-3: without `server.properties` the error hints at starting the server once.
    #[test]
    fn missing_server_properties_hints_at_starting_the_server() {
        let directory = temporary_directory();

        let error = read_level_name(directory.path()).unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some("start the server once so that it creates server.properties")
        );
    }

    /// dp-3: a `server.properties` without `level-name` is an error.
    #[test]
    fn server_properties_without_level_name_is_an_error() {
        assert!(matches!(
            level_name_of("motd=A\n"),
            Err(Error::NoLevelName { .. })
        ));
    }

    /// dp-3: an empty `level-name` is an error whose hint is to write the world's path.
    #[test]
    fn empty_level_name_hints_at_writing_the_world_path() {
        let error = level_name_of("level-name=\n").unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some(
                "write the path of the world directory as level-name in \
                 /server/server.properties"
            )
        );
    }

    /// 「server.properties の読み取り」 方法: a `server.properties` that is not UTF-8 is an error.
    #[test]
    fn server_properties_that_is_not_utf8_is_an_error() {
        let directory = temporary_directory();
        fs::write(
            directory.path().join("server.properties"),
            b"level-name=\xff\n",
        )
        .unwrap();

        assert!(matches!(
            read_level_name(directory.path()),
            Err(Error::NotUtf8 { .. })
        ));
    }

    /// 「server.properties の読み取り」 方法: an escape that cannot be undone is an error naming the
    /// file.
    #[test]
    fn invalid_escape_is_an_error_naming_the_file() {
        let error = level_name_of("level-name=\\uzzzz\n").unwrap_err();

        assert!(
            error.to_string().contains("/server/server.properties"),
            "{error}"
        );
    }
}
