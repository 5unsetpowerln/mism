use std::io::{self, IsTerminal};
use std::os::fd::{AsFd, BorrowedFd};
use std::string::FromUtf8Error;

use nix::errno::Errno;
use nix::poll::{PollFd, PollFlags, PollTimeout};

use crate::interruption::{self, Signal};
use crate::output;

/// The same interval at which `mism new` checks the flag while it waits for the server.
const POLL_INTERVAL_MILLISECONDS: u8 = 100;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Output(#[from] output::Error),
    #[error("failed to wait for an answer on stdin")]
    Poll(#[source] Errno),
    #[error("failed to read the answer from stdin")]
    Read(#[source] Errno),
    #[error("the answer read from stdin is not UTF-8")]
    NotUtf8(#[source] FromUtf8Error),
    #[error("interrupted by {0}")]
    Interrupted(Signal),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Output(error) => error.hint(),
            Error::Poll(_) | Error::Read(_) | Error::NotUtf8(_) | Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interrupted(signal) => Some(*signal),
            Error::Output(_) | Error::Poll(_) | Error::Read(_) | Error::NotUtf8(_) => None,
        }
    }
}

pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

/// Writes `question` and reads one answer up to a newline from stdin, without the newline. The
/// answer is empty when stdin ends before any byte.
pub fn ask(question: &str) -> Result<String, Error> {
    output::write_prompt(question)?;
    let line = read_line(io::stdin().as_fd(), interruption::received)?;
    // Before the answer is checked, so that the error for one that is not UTF-8 starts on a line
    // of its own.
    end_question_line()?;
    answer_of(line)
}

/// Steps 2 and 3: the bytes up to a newline or the end of input, without the newline. Reads byte
/// by byte from the descriptor itself: a buffer such as `Stdin`'s could take in the next answer,
/// which `poll` would then not see.
fn read_line(input: BorrowedFd, received_signal: fn() -> Option<Signal>) -> Result<Vec<u8>, Error> {
    let mut line = Vec::new();
    let mut byte = [0];
    loop {
        wait_until_readable(input, received_signal)?;
        match nix::unistd::read(input, &mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => line.push(byte[0]),
            // The next wait checks the flag.
            Err(Errno::EINTR) => {}
            Err(errno) => return Err(Error::Read(errno)),
        }
    }
    Ok(line)
}

/// Step 4.
fn answer_of(line: Vec<u8>) -> Result<String, Error> {
    String::from_utf8(line).map_err(Error::NotUtf8)
}

/// Step 5: a terminal that is both stdin and stdout shows the typed newline on stdout itself;
/// otherwise the next output would continue the question's line.
fn end_question_line() -> Result<(), Error> {
    if stdin_is_terminal() && io::stdout().is_terminal() {
        return Ok(());
    }
    Ok(output::end_question_line()?)
}

/// Step 2: a read from a terminal is restarted after a signal, but `poll` returns, so mism waits
/// in `poll` and checks the flag whenever it returns.
fn wait_until_readable(
    input: BorrowedFd,
    received_signal: fn() -> Option<Signal>,
) -> Result<(), Error> {
    loop {
        let mut descriptors = [PollFd::new(input, PollFlags::POLLIN)];
        let polled = nix::poll::poll(
            &mut descriptors,
            PollTimeout::from(POLL_INTERVAL_MILLISECONDS),
        );
        if let Some(signal) = received_signal() {
            return Err(Error::Interrupted(signal));
        }
        match polled {
            Ok(0) | Err(Errno::EINTR) => {}
            Ok(_) => return Ok(()),
            Err(errno) => return Err(Error::Poll(errno)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::os::fd::AsFd;

    use super::{Error, answer_of, read_line};
    use crate::interruption::Signal;

    fn no_signal() -> Option<Signal> {
        None
    }

    /// The answer read from a pipe to which `written` was written before it was closed.
    fn answer_from_closed_pipe(written: &[u8]) -> Result<String, Error> {
        let (reader, mut writer) = io::pipe().unwrap();
        writer.write_all(written).unwrap();
        drop(writer);
        read_line(reader.as_fd(), no_signal).and_then(answer_of)
    }

    /// d-prompt-1
    #[test]
    fn interruption_while_waiting_ends_the_wait() {
        let (reader, _writer) = io::pipe().unwrap();

        let result = read_line(reader.as_fd(), || Some(Signal::Interrupt));

        assert!(matches!(result, Err(Error::Interrupted(Signal::Interrupt))));
    }

    /// d-prompt-2
    #[test]
    fn answers_typed_ahead_on_a_terminal_are_read_one_per_question() {
        let terminal = nix::pty::openpty(None, None).unwrap();
        let mut master = std::fs::File::from(terminal.master);
        master.write_all(b"first\nsecond\n").unwrap();

        let lines = [
            read_line(terminal.slave.as_fd(), no_signal).unwrap(),
            read_line(terminal.slave.as_fd(), no_signal).unwrap(),
        ];

        assert_eq!(lines, [b"first".to_vec(), b"second".to_vec()]);
    }

    /// 「標準入力からの回答の読み取り」 方法 4: the newline is removed, spaces are kept.
    #[test]
    fn answer_is_the_line_without_its_newline() {
        assert_eq!(answer_from_closed_pipe(b" yes \nno\n").unwrap(), " yes ");
    }

    /// 「標準入力からの回答の読み取り」 方法 3: the end of input ends an answer without newline.
    #[test]
    fn end_of_input_ends_the_answer() {
        assert_eq!(answer_from_closed_pipe(b"yes").unwrap(), "yes");
    }

    /// 「標準入力からの回答の読み取り」 方法 4: no byte before the end of input is an empty answer.
    #[test]
    fn closed_input_gives_an_empty_answer() {
        assert_eq!(answer_from_closed_pipe(b"").unwrap(), "");
    }

    /// 「標準入力からの回答の読み取り」 方法 4: an answer that is not UTF-8 is an error.
    #[test]
    fn answer_that_is_not_utf8_is_an_error() {
        assert!(matches!(
            answer_from_closed_pipe(b"\xff\n"),
            Err(Error::NotUtf8(_))
        ));
    }
}
