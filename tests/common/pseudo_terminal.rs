use std::fs::File;
use std::io::{ErrorKind, Read};
use std::os::fd::OwnedFd;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::pty::{OpenptyResult, openpty};

const READ_DEADLINE: Duration = Duration::from_secs(30);

/// Opens a pseudo-terminal. Both descriptors are closed on exec, so that a process another test
/// starts meanwhile does not keep the slave side open.
pub fn open() -> OpenptyResult {
    let terminal = openpty(None, None).unwrap();
    for descriptor in [&terminal.master, &terminal.slave] {
        fcntl(descriptor, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).unwrap();
    }
    terminal
}

/// Everything written to the slave side until every descriptor of it is closed, with each
/// `\r\n` that the terminal made of a `\n` turned back into `\n`. Fails the test if the slave side
/// stays open for 30 seconds.
pub fn read_until_closed(master: OwnedFd) -> String {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || sender.send(read_to_hangup(File::from(master))));
    let written = receiver
        .recv_timeout(READ_DEADLINE)
        .unwrap_or_else(|_| panic!("the terminal stayed open for {READ_DEADLINE:?}"));
    String::from_utf8(written).unwrap().replace("\r\n", "\n")
}

fn read_to_hangup(mut master: File) -> Vec<u8> {
    let mut written = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        match master.read(&mut buffer) {
            Ok(0) => return written,
            Ok(count) => written.extend_from_slice(&buffer[..count]),
            // Linux reports EIO on the master side once the slave side is closed.
            Err(error) if error.raw_os_error() == Some(Errno::EIO as i32) => return written,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => panic!("failed to read the terminal: {error}"),
        }
    }
}
