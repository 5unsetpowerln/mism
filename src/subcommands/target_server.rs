use crate::server_directory::{self, ServerDirectory};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the current directory has no mism.toml, so it is not a server directory")]
    NoTargetServer,
    #[error(transparent)]
    ServerDirectory(#[from] server_directory::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::NoTargetServer => {
                Some("run the command in a server directory that `mism new` created".to_owned())
            }
            Error::ServerDirectory(error) => error.hint(),
        }
    }
}

/// Steps 1 and 3 of 「mism plugin と mism datapack の対象のサーバーと共通の手順」: the current
/// directory as the target server, `None` when it has no `mism.toml`; `versions` and `inspect` go
/// on without one (plg-2, dp-2).
pub fn current() -> Result<Option<ServerDirectory>, Error> {
    Ok(ServerDirectory::current()?)
}

/// Step 2: `place`, `install`, `remove`, and `list` without `--global` need the target server
/// (plg-1, dp-1).
pub fn required() -> Result<ServerDirectory, Error> {
    current()?.ok_or(Error::NoTargetServer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// plg-1: the error without a target server hints at `mism new`.
    #[test]
    fn missing_target_server_hints_at_mism_new() {
        assert_eq!(
            Error::NoTargetServer.hint().as_deref(),
            Some("run the command in a server directory that `mism new` created")
        );
    }
}
