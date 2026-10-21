use std::fs::File;
use std::io::Write;
use std::process::{Child, Command, Stdio};

use super::pseudo_terminal;

/// What mism's stdin is connected to.
pub enum StdinConnection {
    /// A pipe that stays open and receives nothing, so mism waiting for input never gets any.
    NeverClosing,
    /// A pipe that receives these bytes and is then closed: stdin that is not a terminal
    /// (fetch-2, plg-11, inst-7).
    ClosedAfter(Vec<u8>),
    /// The slave side of a pseudo-terminal on which these bytes were typed ahead: stdin that is a
    /// terminal (plg-11, inst-5). The terminal's canonical mode hands each line to one read, so
    /// the test need not wait for mism's question before typing.
    Terminal(Vec<u8>),
}

impl StdinConnection {
    /// Connects mism's stdin before mism starts. Returns the master side of a terminal, which
    /// must stay open as long as mism runs.
    pub fn connect(&self, command: &mut Command) -> Option<File> {
        match self {
            StdinConnection::NeverClosing | StdinConnection::ClosedAfter(_) => {
                command.stdin(Stdio::piped());
                None
            }
            StdinConnection::Terminal(typed) => {
                let terminal = pseudo_terminal::open();
                let mut master = File::from(terminal.master);
                master.write_all(typed).unwrap();
                command.stdin(terminal.slave);
                Some(master)
            }
        }
    }

    /// Writes the bytes of `ClosedAfter` to the started mism and closes the pipe.
    pub fn feed(&self, child: &mut Child) {
        if let StdinConnection::ClosedAfter(bytes) = self {
            let mut pipe = child.stdin.take().expect("stdin is a pipe");
            // mism may exit before it reads; the bytes then have no reader, which is fine.
            let _ = pipe.write_all(bytes);
        }
    }
}
