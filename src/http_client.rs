use std::env;
use std::io::{self, Read, Write};
use std::num::ParseFloatError;
use std::time::{Duration, TryFromFloatSecsError};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use ureq::config::Config;
use ureq::http::{Response, StatusCode};
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};
use ureq::{Agent, Body};

use crate::interruption::{self, Signal};

const USER_AGENT: &str = concat!("mism/", env!("CARGO_PKG_VERSION"), " (me@5unset.me)");
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const JSON_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const IDLE_TIMEOUT_VARIABLE: &str = "MISM_DOWNLOAD_IDLE_TIMEOUT";
const DOWNLOAD_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{IDLE_TIMEOUT_VARIABLE} is not a number of seconds: {value:?}")]
    IdleTimeoutNotANumber {
        value: String,
        #[source]
        source: ParseFloatError,
    },
    #[error("{IDLE_TIMEOUT_VARIABLE} is out of range for a number of seconds: {value:?}")]
    IdleTimeoutOutOfRange {
        value: String,
        #[source]
        source: TryFromFloatSecsError,
    },
    #[error("request to {url} failed")]
    Request {
        url: String,
        #[source]
        source: ureq::Error,
    },
    #[error("{url} responded with HTTP status {status}{}", describe_message(.message))]
    Status {
        url: String,
        status: u16,
        message: Option<String>,
    },
    #[error("the response from {url} is not the expected JSON")]
    InvalidJson {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write the data downloaded from {url}")]
    Write {
        url: String,
        #[source]
        source: io::Error,
    },
    #[error("interrupted by {0}")]
    Interrupted(Signal),
}

fn describe_message(message: &Option<String>) -> String {
    message
        .as_ref()
        .map(|message| format!(": {message}"))
        .unwrap_or_default()
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Request { .. } | Error::Status { .. } => {
                Some("check your network connection".to_owned())
            }
            Error::IdleTimeoutNotANumber { .. }
            | Error::IdleTimeoutOutOfRange { .. }
            | Error::InvalidJson { .. }
            | Error::Write { .. }
            | Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interrupted(signal) => Some(*signal),
            Error::IdleTimeoutNotANumber { .. }
            | Error::IdleTimeoutOutOfRange { .. }
            | Error::Request { .. }
            | Error::Status { .. }
            | Error::InvalidJson { .. }
            | Error::Write { .. } => None,
        }
    }
}

#[derive(Clone)]
pub struct HttpClient {
    json_agent: Agent,
    download_agent: Agent,
    /// `interruption::received`, except in unit tests, which must not touch the process-wide flag.
    received_signal: fn() -> Option<Signal>,
}

impl HttpClient {
    pub fn new() -> Result<HttpClient, Error> {
        Ok(HttpClient::with_idle_timeout(
            idle_timeout_from_environment()?,
        ))
    }

    fn with_idle_timeout(idle_timeout: Duration) -> HttpClient {
        HttpClient {
            json_agent: json_agent(),
            download_agent: download_agent(idle_timeout),
            received_signal: interruption::received,
        }
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Error> {
        let body = self
            .receive_json_body(url)
            .map_err(|error| self.attribute_to_interruption(error))?;
        serde_json::from_slice(&body).map_err(|source| Error::InvalidJson {
            url: url.to_owned(),
            source,
        })
    }

    pub fn download(&self, url: &str, sink: &mut dyn Write) -> Result<(), Error> {
        let mut response = get_successful_response(&self.download_agent, url)
            .map_err(|error| self.attribute_to_interruption(error))?;
        self.copy_until_interrupted(url, &mut response.body_mut().as_reader(), sink)
    }

    fn receive_json_body(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut response = get_successful_response(&self.json_agent, url)?;
        response
            .body_mut()
            .read_to_vec()
            .map_err(|source| request_failed(url, source))
    }

    /// Streams `body` into `sink`, checking the interruption flag after every read.
    fn copy_until_interrupted(
        &self,
        url: &str,
        body: &mut dyn Read,
        sink: &mut dyn Write,
    ) -> Result<(), Error> {
        let mut chunk = vec![0; DOWNLOAD_CHUNK_SIZE];
        loop {
            let read = body.read(&mut chunk);
            // Checked before the read's result, which may be the EINTR failure the signal caused.
            if let Some(signal) = (self.received_signal)() {
                return Err(Error::Interrupted(signal));
            }
            let length = read.map_err(|source| request_failed(url, ureq::Error::from(source)))?;
            if length == 0 {
                return Ok(());
            }
            sink.write_all(&chunk[..length])
                .map_err(|source| Error::Write {
                    url: url.to_owned(),
                    source,
                })?;
        }
    }

    /// A signal can make a blocked socket read fail with EINTR, so a failure while the interruption
    /// flag is set is reported as the interruption.
    fn attribute_to_interruption(&self, error: Error) -> Error {
        (self.received_signal)().map_or(error, Error::Interrupted)
    }
}

fn idle_timeout_from_environment() -> Result<Duration, Error> {
    match env::var_os(IDLE_TIMEOUT_VARIABLE) {
        None => Ok(DEFAULT_IDLE_TIMEOUT),
        Some(value) => parse_idle_timeout(&value.to_string_lossy()),
    }
}

fn parse_idle_timeout(value: &str) -> Result<Duration, Error> {
    let seconds = value
        .parse()
        .map_err(|source| Error::IdleTimeoutNotANumber {
            value: value.to_owned(),
            source,
        })?;
    Duration::try_from_secs_f64(seconds).map_err(|source| Error::IdleTimeoutOutOfRange {
        value: value.to_owned(),
        source,
    })
}

fn agent_config(total_timeout: Option<Duration>) -> Config {
    Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(total_timeout)
        // The body of a non-200 response is read for its `message`.
        .http_status_as_error(false)
        .build()
}

fn json_agent() -> Agent {
    Agent::new_with_config(agent_config(Some(JSON_TOTAL_TIMEOUT)))
}

fn download_agent(idle_timeout: Duration) -> Agent {
    Agent::with_parts(
        agent_config(None),
        DefaultConnector::new().chain(IdleTimeoutConnector { idle_timeout }),
        DefaultResolver::default(),
    )
}

fn get_successful_response(agent: &Agent, url: &str) -> Result<Response<Body>, Error> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|source| request_failed(url, source))?;
    if response.status() == StatusCode::OK {
        Ok(response)
    } else {
        Err(unsuccessful_status(url, &mut response))
    }
}

fn unsuccessful_status(url: &str, response: &mut Response<Body>) -> Error {
    Error::Status {
        url: url.to_owned(),
        status: response.status().as_u16(),
        message: error_message(response.body_mut()),
    }
}

#[derive(Deserialize)]
struct ErrorResponse {
    message: String,
}

/// The `message` field of an error response. It only adds detail to the status, so a body that
/// cannot be read or is not such JSON gives `None`.
fn error_message(body: &mut Body) -> Option<String> {
    let bytes = body.read_to_vec().ok()?;
    serde_json::from_slice::<ErrorResponse>(&bytes)
        .ok()
        .map(|response| response.message)
}

fn request_failed(url: &str, source: ureq::Error) -> Error {
    Error::Request {
        url: url.to_owned(),
        source,
    }
}

/// Wraps each connection the default connectors make in an `IdleTimeoutTransport`.
#[derive(Debug)]
struct IdleTimeoutConnector {
    idle_timeout: Duration,
}

impl<In: Transport> Connector<In> for IdleTimeoutConnector {
    type Out = IdleTimeoutTransport<In>;

    fn connect(
        &self,
        _details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| IdleTimeoutTransport {
            inner,
            idle_timeout: self.idle_timeout,
        }))
    }
}

/// Limits every wait for incoming data to `idle_timeout`, which ureq 3.4 cannot configure: its
/// timeouts limit only the total time of each phase of a request.
#[derive(Debug)]
struct IdleTimeoutTransport<T> {
    inner: T,
    idle_timeout: Duration,
}

impl<T: Transport> Transport for IdleTimeoutTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let idle_timeout = self.idle_timeout.into();
        if timeout.after <= idle_timeout {
            return self.inner.await_input(timeout);
        }
        let clamped = NextTimeout {
            after: idle_timeout,
            reason: timeout.reason,
        };
        self.inner
            .await_input(clamped)
            .map_err(|error| match error {
                ureq::Error::Timeout(_) => ureq::Error::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "no data arrived for {} seconds",
                        self.idle_timeout.as_secs_f64()
                    ),
                )),
                other => other,
            })
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;

    use super::*;

    /// Long enough that no test that does not wait for it reaches it.
    const LONG_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

    /// A local HTTP server for one request. It records the request head, writes each piece of the
    /// response after its pause, and then holds the connection open until the client closes it.
    struct FakeServer {
        url: String,
        request_head: mpsc::Receiver<String>,
    }

    impl FakeServer {
        fn start(pieces: Vec<(Duration, Vec<u8>)>) -> FakeServer {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/resource", listener.local_addr().unwrap());
            let (sender, request_head) = mpsc::channel();
            thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_nodelay(true).unwrap();
                sender.send(read_request_head(&stream)).unwrap();
                for (pause, bytes) in pieces {
                    thread::sleep(pause);
                    if stream.write_all(&bytes).is_err() {
                        return; // The client has closed the connection.
                    }
                }
                let _ = io::copy(&mut stream, &mut io::sink());
            });
            FakeServer { url, request_head }
        }

        fn request_head(&self) -> String {
            self.request_head.recv().unwrap()
        }
    }

    fn read_request_head(stream: &TcpStream) -> String {
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.trim_end().is_empty() {
                return head;
            }
            head.push_str(&line);
        }
    }

    fn header_value(head: &str, name: &str) -> Option<String> {
        head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }

    fn response_head(status: &str, content_length: usize) -> Vec<u8> {
        format!("HTTP/1.1 {status}\r\nContent-Length: {content_length}\r\n\r\n").into_bytes()
    }

    fn response(status: &str, body: &[u8]) -> Vec<u8> {
        [response_head(status, body.len()), body.to_vec()].concat()
    }

    fn immediately(bytes: Vec<u8>) -> (Duration, Vec<u8>) {
        (Duration::ZERO, bytes)
    }

    fn client(idle_timeout: Duration) -> HttpClient {
        HttpClient {
            received_signal: || None,
            ..HttpClient::with_idle_timeout(idle_timeout)
        }
    }

    /// 「HTTP クライアント」 方法: every request carries the User-Agent `mism/<version> (me@5unset.me)`.
    #[test]
    fn request_carries_user_agent() {
        let server = FakeServer::start(vec![immediately(response("200 OK", b"{}"))]);
        let _: serde_json::Value = client(LONG_IDLE_TIMEOUT).get_json(&server.url).unwrap();
        assert_eq!(
            header_value(&server.request_head(), "user-agent").as_deref(),
            Some(concat!(
                "mism/",
                env!("CARGO_PKG_VERSION"),
                " (me@5unset.me)"
            ))
        );
    }

    /// 「HTTP クライアント」 方法: the error for a non-200 status names the URL, the status, and the
    /// JSON `message`. The body was returned by fill.papermc.io on 2026-10-20.
    #[test]
    fn unsuccessful_status_error_names_url_status_and_message() {
        let body = br#"{"ok":false,"error":"version_not_found","message":"No version was found with the given identifier."}"#;
        let server = FakeServer::start(vec![immediately(response("404 Not Found", body))]);
        let error = client(LONG_IDLE_TIMEOUT)
            .get_json::<serde_json::Value>(&server.url)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "{} responded with HTTP status 404: No version was found with the given identifier.",
                server.url
            )
        );
    }

    /// 「HTTP クライアント」 方法: a download fails when no data arrives for the idle timeout.
    #[test]
    fn download_fails_when_no_data_arrives_for_idle_timeout() {
        let server = FakeServer::start(vec![immediately(response_head("200 OK", 4))]);
        let result = client(Duration::from_millis(300)).download(&server.url, &mut Vec::new());
        assert!(matches!(
            result,
            Err(Error::Request { source: ureq::Error::Io(ref error), .. })
                if error.kind() == io::ErrorKind::TimedOut
        ));
    }

    /// 「HTTP クライアント」 方法: a download whose pieces arrive within the idle timeout completes,
    /// even when the whole download takes longer than the idle timeout.
    #[test]
    fn download_completes_when_pieces_arrive_within_idle_timeout() {
        let body = b"abcdefgh";
        let pieces = body
            .iter()
            .map(|byte| (Duration::from_millis(100), vec![*byte]));
        let server = FakeServer::start(
            [immediately(response_head("200 OK", body.len()))]
                .into_iter()
                .chain(pieces)
                .collect(),
        );
        let mut sink = Vec::new();
        client(Duration::from_millis(500))
            .download(&server.url, &mut sink)
            .unwrap();
        assert_eq!(sink, body);
    }

    /// 「HTTP クライアント」 方法: a download stops when the interruption flag is set.
    #[test]
    fn download_stops_when_interruption_flag_is_set() {
        let server = FakeServer::start(vec![immediately(response("200 OK", b"jar"))]);
        let client = HttpClient {
            received_signal: || Some(Signal::Interrupt),
            ..client(LONG_IDLE_TIMEOUT)
        };
        let result = client.download(&server.url, &mut Vec::new());
        assert!(matches!(result, Err(Error::Interrupted(Signal::Interrupt))));
    }

    /// 「全体の構成」: `MISM_DOWNLOAD_IDLE_TIMEOUT` is a number of seconds and may have a fraction.
    #[test]
    fn idle_timeout_accepts_fractional_seconds() {
        assert_eq!(
            parse_idle_timeout("0.5").unwrap(),
            Duration::from_millis(500)
        );
    }

    /// 「全体の構成」: a `MISM_DOWNLOAD_IDLE_TIMEOUT` that is not a number is an error.
    #[test]
    fn idle_timeout_rejects_non_number() {
        assert!(matches!(
            parse_idle_timeout("soon"),
            Err(Error::IdleTimeoutNotANumber { .. })
        ));
    }
}
