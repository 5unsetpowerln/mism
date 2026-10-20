use std::io::{self, IsTerminal, Write};

const PROGRAM_NAME: &str = "mism";
/// SGR 34, the blue foreground color, whose actual shade the terminal's theme decides.
const BLUE: &str = "\x1b[34m";
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

/// Writes each line of `text` to stdout after `mism: ` (out-1).
pub fn write_to_stdout(text: &str) -> Result<(), Error> {
    write_and_flush(&prefixed_lines(text))
}

/// Writes to stdout the line announcing that the output of `program` follows, and an empty line
/// (out-2).
pub fn write_external_output_start(program: &str) -> Result<(), Error> {
    write_and_flush(&format!(
        "{}\n",
        marker_line(&format!("output of {program} follows"))
    ))
}

/// Writes to stdout an empty line, and the line telling that the output of `program` ended
/// (out-2).
pub fn write_external_output_end(program: &str) -> Result<(), Error> {
    write_and_flush(&format!(
        "\n{}",
        marker_line(&format!("output of {program} ended"))
    ))
}

/// `text` after `mism: ` as one line, with `mism` in blue when stdout is a terminal (out-3).
fn marker_line(text: &str) -> String {
    if io::stdout().is_terminal() {
        format!("{BLUE}{PROGRAM_NAME}{RESET}: {text}\n")
    } else {
        format!("{PROGRAM_NAME}: {text}\n")
    }
}

/// Each line of `text` after `mism: `, each ending with a newline.
fn prefixed_lines(text: &str) -> String {
    text.lines()
        .map(|line| format!("{PROGRAM_NAME}: {line}\n"))
        .collect()
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
            prefixed_lines("`java -version` failed:\nError: no JVM"),
            "mism: `java -version` failed:\nmism: Error: no JVM\n"
        );
    }
}
