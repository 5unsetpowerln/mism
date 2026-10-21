//! Reading what a `mism plugin` subcommand wrote to stdout.

/// The fields of the record mism wrote last: the last line without `mism: `, split at tabs
/// (plg-9).
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

/// The line `mism: <name>\t...` of an inspection value or judgement (insp-2, insp-3), without
/// `mism: `.
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
