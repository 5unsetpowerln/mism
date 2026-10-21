//! Reading what a `mism plugin` or `mism datapack` subcommand wrote to stdout.

use super::mism_process::Output;

/// The fields of the record mism wrote last: the last line without `mism: `, split at tabs
/// (plg-9, dp-12).
pub fn last_record(stdout: &str) -> Vec<&str> {
    let last_line = stdout.lines().last().expect("mism wrote a line");
    last_line
        .strip_prefix("mism: ")
        .unwrap_or_else(|| panic!("the line lacks `mism: `: {last_line}"))
        .split('\t')
        .collect()
}

/// Whether stdout has the line `mism: <text>`.
pub fn has_line(stdout: &str, text: &str) -> bool {
    let line = format!("mism: {text}");
    stdout.lines().any(|written| written == line)
}

/// The line `mism: <name>\t...` of an inspection value or judgement (insp-2, insp-3, dinsp-2,
/// dinsp-3), without `mism: `.
pub fn described<'a>(stdout: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("mism: {name}\t");
    stdout
        .lines()
        .find(|line| line.starts_with(&start))
        .map(|line| &line["mism: ".len()..])
}

/// The index of the first line that is `mism: <text>`, for checking the order of lines.
pub fn line_index(stdout: &str, text: &str) -> Option<usize> {
    let line = format!("mism: {text}");
    stdout.lines().position(|written| written == line)
}

/// Whether mism reported an error as err-9 says, in whatever words: it exited with status 1, its
/// last line on stdout starts with `mism: `, and it wrote nothing to stderr (out-4).
pub fn reported_an_error(output: &Output) -> bool {
    output.exit_code == 1
        && output.stderr.is_empty()
        && output
            .stdout
            .lines()
            .last()
            .is_some_and(|line| line.starts_with("mism: "))
}
