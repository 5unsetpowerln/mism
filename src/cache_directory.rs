use std::ffi::OsString;
use std::io;
use std::path::{self, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("neither XDG_CACHE_HOME nor HOME is set, so the cache directory cannot be determined")]
    HomeNotSet,
    #[error("failed to make the cache directory {} an absolute path", path.display())]
    MakeAbsolute {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::HomeNotSet | Error::MakeAbsolute { .. } => None,
        }
    }
}

/// The absolute path of `$XDG_CACHE_HOME/mism` when `XDG_CACHE_HOME` is set and not empty
/// (cache-1), otherwise of `~/.cache/mism` (cache-2).
pub fn path() -> Result<PathBuf, Error> {
    let path = path_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))?;
    path::absolute(&path).map_err(|source| Error::MakeAbsolute { path, source })
}

fn path_from(xdg_cache_home: Option<OsString>, home: Option<OsString>) -> Result<PathBuf, Error> {
    match xdg_cache_home.filter(|value| !value.is_empty()) {
        Some(xdg_cache_home) => Ok(PathBuf::from(xdg_cache_home).join("mism")),
        None => Ok(PathBuf::from(home.ok_or(Error::HomeNotSet)?)
            .join(".cache")
            .join("mism")),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::{Error, path_from};

    /// cache-1: XDG_CACHE_HOME selects the cache directory.
    #[test]
    fn xdg_cache_home_selects_the_cache_directory() {
        let directory = path_from(Some("/xdg".into()), Some("/home/user".into()));

        assert_eq!(directory.unwrap(), Path::new("/xdg/mism"));
    }

    /// cache-2: without XDG_CACHE_HOME the cache directory is under HOME.
    #[test]
    fn home_cache_is_used_without_xdg_cache_home() {
        let directory = path_from(None, Some("/home/user".into()));

        assert_eq!(directory.unwrap(), Path::new("/home/user/.cache/mism"));
    }

    /// cache-2: an empty XDG_CACHE_HOME counts as unset.
    #[test]
    fn empty_xdg_cache_home_counts_as_unset() {
        let directory = path_from(Some(OsString::new()), Some("/home/user".into()));

        assert_eq!(directory.unwrap(), Path::new("/home/user/.cache/mism"));
    }

    /// 「キャッシュのディレクトリとハッシュの計算」 方法: with neither variable set there is no
    /// fallback.
    #[test]
    fn missing_home_is_an_error() {
        assert!(matches!(path_from(None, None), Err(Error::HomeNotSet)));
    }
}
