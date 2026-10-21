use crate::modrinth_api::selection::{AskError, Asker};
use crate::prompt;

/// Asks through stdin only when it is a terminal and the user did not give `--yes` (plg-11,
/// dp-5).
pub struct TerminalAsker {
    yes: bool,
}

impl TerminalAsker {
    pub fn new(yes: bool) -> TerminalAsker {
        TerminalAsker { yes }
    }
}

impl Asker for TerminalAsker {
    fn ask(&mut self, question: &str) -> Result<Option<String>, AskError> {
        if self.yes || !prompt::stdin_is_terminal() {
            return Ok(None);
        }
        prompt::ask(question)
            .map(Some)
            .map_err(|error| match error.interrupted_by() {
                Some(signal) => AskError::Interrupted(signal),
                None => AskError::Failed(Box::new(error)),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// plg-11: with `--yes` the user is not asked.
    #[test]
    fn yes_means_the_user_cannot_be_asked() {
        assert_eq!(TerminalAsker::new(true).ask("question: ").unwrap(), None);
    }
}
