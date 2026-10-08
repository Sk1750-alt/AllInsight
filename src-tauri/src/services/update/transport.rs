//! The only code in AllInsight that talks to the internet.
//!
//! What a request carries is decided in one place, [`request_headers`], and
//! a test pins it: a fixed User-Agent naming the product, and nothing else.
//! No query string, no cookies (ureq's cookie feature is not enabled), no
//! installation id, no version-specific path. Every copy of AllInsight asks for
//! the same URL with the same bytes.
//!
//! Redirects are followed by hand rather than by the library, so that each hop
//! goes through the same HTTPS and allowed-host check as the first request.

use std::io::{Read, Write};
use std::time::Duration;

use super::config::{check_url, UrlRefusal};

/// Sent on every request. Deliberately carries no version, platform or id.
pub const USER_AGENT: &str = "AllInsight-Updater";

const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The complete set of headers AllInsight adds to an update request.
pub fn request_headers() -> Vec<(&'static str, &'static str)> {
    vec![("User-Agent", USER_AGENT)]
}

/// Why a request did not produce the bytes asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// No route to the server: no network, DNS failure, connection refused.
    Offline,
    Timeout,
    /// The server answered with an error status.
    Status(u16),
    /// The address, or a redirect, was refused before connecting.
    Refused(UrlRefusal),
    TooManyRedirects,
    TooLarge,
    /// Interrupted part way, or failed to write locally.
    Interrupted(String),
}

impl NetError {
    /// True when the cause is the connection rather than the server or the
    /// content: the case the interface words as "you appear to be offline".
    pub fn is_connectivity(&self) -> bool {
        matches!(self, NetError::Offline | NetError::Timeout)
    }
}

/// Progress callback: bytes so far, and the total when the server said.
pub type Progress<'a> = &'a mut dyn FnMut(u64, Option<u64>);

/// What the updater needs from the network. A trait so the tests can stand in
/// for the server and record every request made.
pub trait Transport: Send + Sync {
    /// Fetch a small document entirely into memory.
    fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError>;
    /// Stream a large file into `sink`.
    fn download(
        &self,
        url: &str,
        sink: &mut dyn Write,
        limit: u64,
        progress: Progress<'_>,
    ) -> Result<u64, NetError>;
}

/// The real transport, over HTTPS with ureq.
pub struct HttpsTransport {
    agent: ureq::Agent,
    allowed_hosts: Vec<String>,
}

impl HttpsTransport {
    pub fn new(allowed_hosts: Vec<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .https_only(true)
            .redirects(0)
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .user_agent(USER_AGENT)
            .build();
        Self {
            agent,
            allowed_hosts,
        }
    }

    /// Issue a GET, following redirects only to allowed HTTPS hosts.
    fn get(&self, url: &str) -> Result<ureq::Response, NetError> {
        let mut current = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            check_url(&current, &self.allowed_hosts).map_err(NetError::Refused)?;
            let mut request = self.agent.get(&current);
            for (name, value) in request_headers() {
                request = request.set(name, value);
            }
            let response = request.call().map_err(classify)?;
            if (300..400).contains(&response.status()) {
                let Some(location) = response.header("location") else {
                    return Err(NetError::Status(response.status()));
                };
                current = resolve(&current, location);
                continue;
            }
            return Ok(response);
        }
        Err(NetError::TooManyRedirects)
    }
}

impl Transport for HttpsTransport {
    fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
        let response = self.get(url)?;
        if let Some(len) = content_length(&response) {
            if len > limit {
                return Err(NetError::TooLarge);
            }
        }
        let mut body = Vec::new();
        response
            .into_reader()
            .take(limit + 1)
            .read_to_end(&mut body)
            .map_err(|e| read_error(&e))?;
        if body.len() as u64 > limit {
            return Err(NetError::TooLarge);
        }
        Ok(body)
    }

    fn download(
        &self,
        url: &str,
        sink: &mut dyn Write,
        limit: u64,
        progress: Progress<'_>,
    ) -> Result<u64, NetError> {
        let response = self.get(url)?;
        let total = content_length(&response);
        if total.is_some_and(|t| t > limit) {
            return Err(NetError::TooLarge);
        }
        copy_with_progress(&mut response.into_reader(), sink, limit, total, progress)
    }
}

/// Copy `reader` into `sink`, reporting progress and enforcing `limit`.
pub fn copy_with_progress(
    reader: &mut dyn Read,
    sink: &mut dyn Write,
    limit: u64,
    total: Option<u64>,
    progress: Progress<'_>,
) -> Result<u64, NetError> {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written: u64 = 0;
    progress(0, total);
    loop {
        let n = reader.read(&mut buffer).map_err(|e| read_error(&e))?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > limit {
            return Err(NetError::TooLarge);
        }
        sink.write_all(&buffer[..n])
            .map_err(|e| NetError::Interrupted(e.to_string()))?;
        progress(written, total);
    }
    if let Some(expected) = total {
        if written != expected {
            return Err(NetError::Interrupted(format!(
                "received {written} of {expected} bytes"
            )));
        }
    }
    Ok(written)
}

fn content_length(response: &ureq::Response) -> Option<u64> {
    response
        .header("content-length")
        .and_then(|v| v.trim().parse().ok())
}

fn read_error(e: &std::io::Error) -> NetError {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => NetError::Timeout,
        _ => NetError::Interrupted(e.to_string()),
    }
}

fn classify(error: ureq::Error) -> NetError {
    match error {
        ureq::Error::Status(code, _) => NetError::Status(code),
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => NetError::Offline,
            ureq::ErrorKind::InsecureRequestHttpsOnly | ureq::ErrorKind::UnknownScheme => {
                NetError::Refused(UrlRefusal::NotHttps)
            }
            ureq::ErrorKind::InvalidUrl => NetError::Refused(UrlRefusal::Malformed),
            ureq::ErrorKind::TooManyRedirects => NetError::TooManyRedirects,
            ureq::ErrorKind::Io => {
                let text = t.to_string().to_ascii_lowercase();
                if text.contains("timed out") || text.contains("timeout") {
                    NetError::Timeout
                } else {
                    NetError::Offline
                }
            }
            _ => NetError::Interrupted(t.to_string()),
        },
    }
}

/// Resolve a redirect target against the URL that produced it.
fn resolve(base: &str, location: &str) -> String {
    let location = location.trim();
    if location.contains("://") {
        return location.to_string();
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("https", base));
    let origin_end = rest.find('/').unwrap_or(rest.len());
    let origin = &rest[..origin_end];
    if let Some(stripped) = location.strip_prefix("//") {
        return format!("{scheme}://{stripped}");
    }
    if location.starts_with('/') {
        return format!("{scheme}://{origin}{location}");
    }
    let path = &rest[origin_end..];
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    format!("{scheme}://{origin}{dir}/{location}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use parking_lot::Mutex;

    /// A canned response for one URL.
    pub type Answer = (String, Result<Vec<u8>, NetError>);
    /// A request as sent: the URL and its headers.
    pub type Recorded = (String, Vec<(String, String)>);

    /// A stand-in server. Each URL maps to a canned answer, and every
    /// request is recorded with the headers the real transport would send.
    #[derive(Default)]
    pub struct FakeServer {
        pub answers: Mutex<Vec<Answer>>,
        pub requests: Mutex<Vec<Recorded>>,
    }

    impl FakeServer {
        pub fn answer(&self, url: &str, result: Result<Vec<u8>, NetError>) {
            self.answers.lock().push((url.to_string(), result));
        }

        fn lookup(&self, url: &str) -> Result<Vec<u8>, NetError> {
            self.requests.lock().push((
                url.to_string(),
                request_headers()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            self.answers
                .lock()
                .iter()
                .find(|(u, _)| u == url)
                .map(|(_, r)| r.clone())
                .unwrap_or(Err(NetError::Status(404)))
        }
    }

    impl Transport for FakeServer {
        fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
            let body = self.lookup(url)?;
            if body.len() as u64 > limit {
                return Err(NetError::TooLarge);
            }
            Ok(body)
        }

        fn download(
            &self,
            url: &str,
            sink: &mut dyn Write,
            limit: u64,
            progress: Progress<'_>,
        ) -> Result<u64, NetError> {
            let body = self.lookup(url)?;
            let total = Some(body.len() as u64);
            copy_with_progress(&mut body.as_slice(), sink, limit, total, progress)
        }
    }

    #[test]
    fn requests_carry_only_the_product_user_agent() {
        let headers = request_headers();
        assert_eq!(headers, vec![("User-Agent", "AllInsight-Updater")]);
        // Nothing that identifies a version, a platform or a person.
        assert!(!USER_AGENT.chars().any(|c| c.is_ascii_digit()));
        assert!(!USER_AGENT.contains('/'));
    }

    #[test]
    fn redirects_resolve_against_the_original() {
        assert_eq!(
            resolve(
                "https://github.com/a/b/latest.json",
                "https://objects.githubusercontent.com/x"
            ),
            "https://objects.githubusercontent.com/x"
        );
        assert_eq!(
            resolve("https://github.com/a/b/c", "/d/e"),
            "https://github.com/d/e"
        );
        assert_eq!(
            resolve("https://github.com/a/b/c", "d"),
            "https://github.com/a/b/d"
        );
        assert_eq!(
            resolve("https://github.com/a", "//evil.example/x"),
            "https://evil.example/x"
        );
    }

    #[test]
    fn a_redirect_to_an_untrusted_host_is_refused_before_connecting() {
        // The real transport checks every hop with check_url; prove the
        // redirect target that resolve produces is refused by it.
        let hosts = vec!["github.com".to_string()];
        let hop = resolve("https://github.com/a", "http://github.com/b");
        assert_eq!(check_url(&hop, &hosts), Err(UrlRefusal::NotHttps));
        let hop = resolve("https://github.com/a", "https://evil.example/b");
        assert!(check_url(&hop, &hosts).is_err());
    }

    #[test]
    fn the_real_transport_refuses_http_without_connecting() {
        let transport = HttpsTransport::new(vec!["github.com".into()]);
        assert_eq!(
            transport.fetch("http://github.com/latest.json", 1024),
            Err(NetError::Refused(UrlRefusal::NotHttps))
        );
        assert!(matches!(
            transport.fetch("https://evil.example/latest.json", 1024),
            Err(NetError::Refused(UrlRefusal::UntrustedHost(_)))
        ));
    }

    #[test]
    fn a_short_download_is_reported_as_interrupted() {
        let data = vec![7u8; 100];
        let mut sink = Vec::new();
        let result = copy_with_progress(
            &mut data.as_slice(),
            &mut sink,
            1000,
            Some(250),
            &mut |_, _| {},
        );
        assert!(matches!(result, Err(NetError::Interrupted(_))));
    }

    #[test]
    fn an_oversized_download_is_stopped() {
        let data = vec![7u8; 5000];
        let mut sink = Vec::new();
        let result =
            copy_with_progress(&mut data.as_slice(), &mut sink, 1000, None, &mut |_, _| {});
        assert_eq!(result, Err(NetError::TooLarge));
        assert!(sink.len() <= 1000 + 64 * 1024);
    }

    #[test]
    fn progress_is_reported_up_to_the_total() {
        let data = vec![1u8; 200_000];
        let mut sink = Vec::new();
        let mut last = (0, None);
        copy_with_progress(
            &mut data.as_slice(),
            &mut sink,
            1 << 20,
            Some(200_000),
            &mut |d, t| last = (d, t),
        )
        .unwrap();
        assert_eq!(last, (200_000, Some(200_000)));
    }
}

/// Talks to the real GitHub, so it is not part of the normal run:
/// `cargo test live_ -- --ignored`. Proves TLS, the redirect handling and
/// error classification work against the actual release host.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn live_release_host_answers_over_tls() {
        let config = super::super::config::UpdateConfig::compiled();
        let transport = HttpsTransport::new(config.allowed_hosts.clone());
        let url = config.metadata_url(super::super::channel::Channel::Stable);
        match transport.fetch(&url, 256 * 1024) {
            Ok(body) => println!("fetched {} bytes from {url}", body.len()),
            Err(NetError::Status(code)) => println!("server answered {code} for {url}"),
            Err(e) => panic!("expected an HTTP answer from {url}, got {e:?}"),
        }
    }
}
