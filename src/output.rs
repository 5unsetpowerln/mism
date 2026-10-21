use std::io::{self, IsTerminal, Write};

const PROGRAM_NAME: &str = "mism";
/// SGR 34, the blue foreground color, whose actual shade the terminal's theme decides.
const BLUE: &str = "\x1b[34m";
/// SGR 33, the yellow foreground color, whose actual shade the terminal's theme decides.
const YELLOW: &str = "\x1b[33m";
/// SGR 0, which ends the color.
const RESET: &str = "\x1b[0m";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to write to stdout")]
    WriteStdout(#[source] io::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::WriteStdout(_) => None,
        }
    }
}

/// Writes each line of `text` to stdout after `mism: `, with `mism:` in blue when stdout is a
/// terminal (out-1, out-3).
pub fn write_to_stdout(text: &str) -> Result<(), Error> {
    write_and_flush(&prefixed_lines(text, &prefix_on_stdout(BLUE)))
}

/// Writes each line of `text` as a warning after `mism: warning: `, with `mism:` in yellow when
/// stdout is a terminal (out-5).
pub fn write_warning(text: &str) -> Result<(), Error> {
    write_and_flush(&prefixed_lines(
        text,
        &warning_prefix(&prefix_on_stdout(YELLOW)),
    ))
}

/// Writes the question that precedes reading an answer from stdin: each line after `mism: `, and
/// no newline after the last one, so that the user types the answer right after it. `mism:` is
/// blue when stdout is a terminal (out-3).
pub fn write_prompt(question: &str) -> Result<(), Error> {
    write_and_flush(&prefixed_prompt(question, &prefix_on_stdout(BLUE)))
}

/// Ends the line of a question after its answer was read, for when no terminal showed the
/// newline the user typed on stdout.
pub fn end_question_line() -> Result<(), Error> {
    write_and_flush("\n")
}

/// One value of a record that a `mism plugin` or `mism datapack` subcommand writes (plg-9,
/// dp-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field<'a> {
    /// A value, or none (`-`).
    Value(Option<&'a str>),
    /// A list, written without duplicates in ascending order of the UTF-8 bytes.
    List(Vec<&'a str>),
    /// A list of Minecraft versions, written without duplicates in the order Modrinth returned.
    MinecraftVersions(Vec<&'a str>),
}

const ABSENT: &str = "-";

/// The fields separated by tabs, the elements of each list separated by `,`, and `-` for a value
/// that is absent and for a list without elements (plg-9). The text has no newline.
pub fn record(fields: &[Field]) -> String {
    fields.iter().map(field_text).collect::<Vec<_>>().join("\t")
}

fn field_text(field: &Field) -> String {
    let elements = match field {
        Field::Value(value) => return value.unwrap_or(ABSENT).to_owned(),
        Field::List(elements) => {
            let mut sorted = elements.clone();
            sorted.sort_unstable();
            sorted.dedup();
            sorted
        }
        Field::MinecraftVersions(elements) => {
            let mut unique: Vec<&str> = Vec::with_capacity(elements.len());
            for element in elements {
                if !unique.contains(element) {
                    unique.push(element);
                }
            }
            unique
        }
    };
    if elements.is_empty() {
        ABSENT.to_owned()
    } else {
        elements.join(",")
    }
}

/// Writes to stdout the line announcing that the output of `program` follows, and an empty line
/// (out-2).
pub fn write_external_output_start(program: &str) -> Result<(), Error> {
    write_and_flush(&format!(
        "{}\n",
        prefixed_lines(
            &format!("output of {program} follows"),
            &prefix_on_stdout(BLUE)
        )
    ))
}

/// Writes to stdout an empty line, and the line telling that the output of `program` ended
/// (out-2).
pub fn write_external_output_end(program: &str) -> Result<(), Error> {
    write_and_flush(&format!(
        "\n{}",
        prefixed_lines(
            &format!("output of {program} ended"),
            &prefix_on_stdout(BLUE)
        )
    ))
}

/// `mism:`, in `color` when stdout is a terminal (out-3, out-5).
fn prefix_on_stdout(color: &str) -> String {
    prefix(color, io::stdout().is_terminal())
}

/// `mism:`, in `color` when `colored`.
fn prefix(color: &str, colored: bool) -> String {
    if colored {
        format!("{color}{PROGRAM_NAME}:{RESET}")
    } else {
        format!("{PROGRAM_NAME}:")
    }
}

/// `prefix` followed by ` warning:` (out-5).
fn warning_prefix(prefix: &str) -> String {
    format!("{prefix} warning:")
}

/// Each line of `text` after `prefix` and a space, each ending with a newline.
fn prefixed_lines(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix} {line}\n"))
        .collect()
}

/// Each line of `question` after `prefix` and a space, the last one without a newline.
fn prefixed_prompt(question: &str, prefix: &str) -> String {
    question
        .split('\n')
        .map(|line| format!("{prefix} {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Writes `text` to stdout and flushes it, so that it precedes whatever the server, which writes
/// to the same stdout, prints afterwards.
fn write_and_flush(text: &str) -> Result<(), Error> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(Error::WriteStdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// out-1: each line of a text of several lines gets the prefix, the last one too when the
    /// text does not end with a newline.
    #[test]
    fn every_line_of_a_text_gets_the_prefix() {
        assert_eq!(
            prefixed_lines(
                "`java -version` failed:\nError: no JVM",
                &prefix(BLUE, false)
            ),
            "mism: `java -version` failed:\nmism: Error: no JVM\n"
        );
    }

    /// out-5: each line of a warning gets `mism: warning: `.
    #[test]
    fn every_line_of_a_warning_gets_the_warning_prefix() {
        assert_eq!(
            prefixed_lines("first\nsecond", &warning_prefix(&prefix(YELLOW, false))),
            "mism: warning: first\nmism: warning: second\n"
        );
    }

    /// out-5: on a terminal, only `mism:` of a warning line is yellow.
    #[test]
    fn colored_warning_has_a_yellow_mism() {
        assert_eq!(
            prefixed_lines("first", &warning_prefix(&prefix(YELLOW, true))),
            "\x1b[33mmism:\x1b[0m warning: first\n"
        );
    }

    /// out-3: on a terminal, only `mism:` of a line is blue.
    #[test]
    fn colored_line_has_a_blue_mism() {
        assert_eq!(
            prefixed_lines("first", &prefix(BLUE, true)),
            "\x1b[34mmism:\x1b[0m first\n"
        );
    }

    /// 「mism 自身の出力」 方法: a question gets the prefix on each line and ends without a
    /// newline.
    #[test]
    fn question_gets_the_prefix_and_no_final_newline() {
        assert_eq!(
            prefixed_prompt(
                "modrinth:a has files with these values of version:\n2.0\nversion: ",
                &prefix(BLUE, false)
            ),
            "mism: modrinth:a has files with these values of version:\nmism: 2.0\nmism: version: "
        );
    }

    /// plg-9: values are separated by tabs; a list's elements are sorted, unique, and separated by
    /// commas.
    #[test]
    fn record_separates_values_by_tabs_and_list_elements_by_commas() {
        assert_eq!(
            record(&[
                Field::Value(Some("modrinth:luckperms")),
                Field::Value(Some("LuckPerms")),
                Field::List(vec!["spigot", "paper", "bukkit", "folia", "paper"]),
            ]),
            "modrinth:luckperms\tLuckPerms\tbukkit,folia,paper,spigot"
        );
    }

    /// plg-9: an absent value is `-`.
    #[test]
    fn absent_value_is_a_hyphen() {
        assert_eq!(
            record(&[Field::Value(None), Field::Value(Some("a"))]),
            "-\ta"
        );
    }

    /// plg-9: a list without elements is `-`.
    #[test]
    fn empty_list_is_a_hyphen() {
        assert_eq!(
            record(&[
                Field::List(Vec::new()),
                Field::MinecraftVersions(Vec::new())
            ]),
            "-\t-"
        );
    }

    /// 「全体の構成」: Minecraft versions keep Modrinth's order without duplicates.
    #[test]
    fn minecraft_versions_keep_their_order_without_duplicates() {
        assert_eq!(
            record(&[Field::MinecraftVersions(vec![
                "1.9", "1.8.9", "26.3", "1.9"
            ])]),
            "1.9,1.8.9,26.3"
        );
    }

    /// 「全体の構成」: lists are sorted by their UTF-8 bytes, so upper case comes first.
    #[test]
    fn list_is_sorted_by_utf8_bytes() {
        assert_eq!(
            record(&[Field::List(vec!["vault", "Vault", "éa", "z"])]),
            "Vault,vault,z,éa"
        );
    }
}
