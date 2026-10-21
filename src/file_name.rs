#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the plugin name {name:?} cannot be used: it is empty, `.`, `..`, or contains NUL")]
    PluginName { name: String },
    #[error(
        "the jar file name {name:?} cannot be used: it is empty, `.`, `..`, or contains `/` or NUL"
    )]
    JarFileName { name: String },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::PluginName { .. } | Error::JarFileName { .. } => None,
        }
    }
}

/// Checks a plugin name from outside mism before it names a directory (plg-6). A `/` passes:
/// the cache directory replaces it.
pub fn check_plugin_name(name: &str) -> Result<(), Error> {
    if is_special(name) || name.contains('\0') {
        return Err(Error::PluginName {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// Checks a jar file name from outside mism before it names a file in a directory.
pub fn check_jar_file_name(name: &str) -> Result<(), Error> {
    if is_special(name) || name.contains(['/', '\0']) {
        return Err(Error::JarFileName {
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
    use super::{check_jar_file_name, check_plugin_name};

    /// plg-6
    #[test]
    fn empty_dot_and_dot_dot_are_not_plugin_names() {
        let accepted: Vec<_> = ["", ".", ".."]
            .into_iter()
            .filter(|name| check_plugin_name(name).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// 「ファイルの名前の検査」 方法: a plugin name with NUL is rejected.
    #[test]
    fn plugin_name_with_nul_is_rejected() {
        assert!(check_plugin_name("Luck\0Perms").is_err());
    }

    /// 「ファイルの名前の検査」 方法: a plugin name with `/` passes.
    #[test]
    fn plugin_name_with_a_slash_passes() {
        assert!(check_plugin_name("A/B").is_ok());
    }

    /// 「ファイルの名前の検査」 方法: an ordinary plugin name passes.
    #[test]
    fn ordinary_plugin_name_passes() {
        assert!(check_plugin_name("LuckPerms").is_ok());
    }

    /// 「ファイルの名前の検査」 方法: empty, `.`, `..`, `/` and NUL make jar file names invalid.
    #[test]
    fn special_jar_file_names_are_rejected() {
        let accepted: Vec<_> = ["", ".", "..", "../server.properties", "a\0.jar"]
            .into_iter()
            .filter(|name| check_jar_file_name(name).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// d-name-2: the name `a/b` answered to the fetch-2 question gives a jar file name with `/`.
    #[test]
    fn jar_file_name_from_an_answer_with_a_slash_is_rejected() {
        assert!(check_jar_file_name(&format!("{}.jar", "a/b")).is_err());
    }

    /// 「ファイルの名前の検査」 方法: an ordinary jar file name passes.
    #[test]
    fn ordinary_jar_file_name_passes() {
        assert!(check_jar_file_name("LuckPerms-Bukkit-5.5.71.jar").is_ok());
    }
}
