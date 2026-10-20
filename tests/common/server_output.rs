/// The line with which mism announces the output of the server started from `jar_name` (out-2).
pub fn start_line(jar_name: &str) -> String {
    format!("mism: output of {jar_name} follows")
}

/// The line with which mism ends the output of the server started from `jar_name` (out-2).
pub fn end_line(jar_name: &str) -> String {
    format!("mism: output of {jar_name} ended")
}

/// What the server printed: the part of `stderr` after the start line and the empty line that
/// follows it, and before the empty line that precedes the end line. `None` if `stderr` lacks
/// either.
pub fn server_output<'a>(stderr: &'a str, jar_name: &str) -> Option<&'a str> {
    let (_, after_start) = stderr.split_once(&format!("{}\n\n", start_line(jar_name)))?;
    let (printed, _) = after_start.split_once(&format!("\n{}\n", end_line(jar_name)))?;
    Some(printed)
}
