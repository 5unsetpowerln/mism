use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

/// A local HTTP server, running in a thread of the test, that answers one GET with a jar sent
/// slowly: wiremock cannot send a response body in pieces.
pub struct SlowJarServer {
    url: String,
}

impl SlowJarServer {
    /// Answers the first request with `jar_bytes` (with `Content-Length`) split into `piece_count`
    /// pieces, sending each piece `pause` after the previous one (the first `pause` after the
    /// response head).
    pub fn start(jar_bytes: &[u8], piece_count: usize, pause: Duration) -> SlowJarServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/jars/slow.jar", listener.local_addr().unwrap());
        let pieces = split(jar_bytes, piece_count);
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            jar_bytes.len()
        );
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).unwrap();
            read_request_head(&stream);
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            for piece in pieces {
                thread::sleep(pause);
                if stream.write_all(&piece).is_err() {
                    return; // mmmm has closed the connection.
                }
            }
        });
        SlowJarServer { url }
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

/// `bytes` split into `count` pieces whose lengths differ by at most 1.
fn split(bytes: &[u8], count: usize) -> Vec<Vec<u8>> {
    assert!(bytes.len() >= count, "fewer bytes than pieces");
    (0..count)
        .map(|index| bytes[index * bytes.len() / count..(index + 1) * bytes.len() / count].to_vec())
        .collect()
}

/// Reads the request line and the headers, up to the empty line that ends them.
fn read_request_head(stream: &TcpStream) {
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.trim_end().is_empty() {
            return;
        }
    }
}
