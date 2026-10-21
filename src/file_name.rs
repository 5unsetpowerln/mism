// The callers tell in their own messages what a name names (e.g. a plugin name): this module
// depends on no other one and knows only the rules.

const NAME_RULE: &str = "it is empty, `.`, `..`, or contains NUL";
const FILE_NAME_RULE: &str = "it is empty, `.`, `..`, or contains `/` or NUL";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the name {name:?} cannot be used: {NAME_RULE}")]
    Name { name: String },
    #[error("the file name {name:?} cannot be used: {FILE_NAME_RULE}")]
    FileName { name: String },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Name { .. } | Error::FileName { .. } => None,
        }
    }

    /// The name that cannot be used.
    pub fn name(&self) -> &str {
        match self {
            Error::Name { name } | Error::FileName { name } => name,
        }
    }

    /// Why the name cannot be used, for the messages of the callers that tell what it names.
    pub fn reason(&self) -> &'static str {
        match self {
            Error::Name { .. } => NAME_RULE,
            Error::FileName { .. } => FILE_NAME_RULE,
        }
    }
}

/// Checks a name from outside mism before it names a directory (plg-6, dp-9). A `/` passes: the
/// cache directory replaces it.
pub fn check_name(name: &str) -> Result<(), Error> {
    if is_special(name) || name.contains('\0') {
        return Err(Error::Name {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// Checks a file name from outside mism before it names a file in a directory.
pub fn check_file_name(name: &str) -> Result<(), Error> {
    if is_special(name) || name.contains(['/', '\0']) {
        return Err(Error::FileName {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// Empty, or a name of the directory itself or of its parent.
fn is_special(name: &str) -> bool {
    matches!(name, "" | "." | "..")
}

#[cfg(test)]
mod tests {
    use super::{check_file_name, check_name};

    /// plg-6, dp-9
    #[test]
    fn empty_dot_and_dot_dot_are_not_names() {
        let accepted: Vec<_> = ["", ".", ".."]
            .into_iter()
            .filter(|name| check_name(name).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// 「ファイルの名前の検査」 方法: a name with NUL is rejected.
    #[test]
    fn name_with_nul_is_rejected() {
        assert!(check_name("Luck\0Perms").is_err());
    }

    /// 「ファイルの名前の検査」 方法: a name with `/` passes.
    #[test]
    fn name_with_a_slash_passes() {
        assert!(check_name("A/B").is_ok());
    }

    /// 「ファイルの名前の検査」 方法: an ordinary name passes.
    #[test]
    fn ordinary_name_passes() {
        assert!(check_name("LuckPerms").is_ok());
    }

    /// 「ファイルの名前の検査」 方法: empty, `.`, `..`, `/` and NUL make file names invalid.
    #[test]
    fn special_file_names_are_rejected() {
        let accepted: Vec<_> = ["", ".", "..", "../server.properties", "a\0.jar"]
            .into_iter()
            .filter(|name| check_file_name(name).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// d-name-2: the name `a/b` answered to the fetch-2 question gives a file name with `/`.
    #[test]
    fn file_name_from_an_answer_with_a_slash_is_rejected() {
        assert!(check_file_name(&format!("{}.jar", "a/b")).is_err());
    }

    /// 「ファイルの名前の検査」 方法: an ordinary file name passes.
    #[test]
    fn ordinary_file_name_passes() {
        assert!(check_file_name("LuckPerms-Bukkit-5.5.71.jar").is_ok());
    }
}
