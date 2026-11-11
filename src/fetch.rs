//! Outbound HTTP for feeds, hardened against SSRF.
//!
//! * only http/https on ports 80, 443, 8080 or 8443, no credentials in the URL;
//! * every hostname is resolved here and *all* resolved addresses must be public;
//!   the connection is then pinned to those addresses (no second DNS lookup, so
//!   DNS rebinding between check and connect is not possible);
//! * redirects are followed manually and each hop is validated the same way;
//! * system proxies are ignored, bodies are capped and requests time out.

use reqwest::{
    StatusCode,
    header::{
        ACCEPT, AUTHORIZATION, CONTENT_TYPE, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
        LOCATION,
    },
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};
use url::{Host, Url};

pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const ALLOWED_PORTS: &[u16] = &[80, 443, 8080, 8443];
pub const USER_AGENT: &str = concat!(
    "LNC-Newsman/",
    env!("CARGO_PKG_VERSION"),
    " (internal RSS/Atom reader; metadata only)"
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    InvalidUrl,
    BlockedAddress,
    Dns,
    Timeout,
    TooLarge,
    TooManyRedirects,
    HttpStatus(u16),
    Network,
}

impl FetchError {
    pub fn code(&self) -> String {
        match self {
            Self::InvalidUrl => "invalid_url".into(),
            Self::BlockedAddress => "blocked_address".into(),
            Self::Dns => "dns_failed".into(),
            Self::Timeout => "timeout".into(),
            Self::TooLarge => "response_too_large".into(),
            Self::TooManyRedirects => "too_many_redirects".into(),
            Self::HttpStatus(code) => format!("http_{code}"),
            Self::Network => "network_error".into(),
        }
    }
}

/// Conditional-request state remembered for one feed.
///
/// `origin` is the origin (scheme + host + port) that issued the validators. They
/// are only ever replayed to that origin, never to a redirect target on another
/// origin, so a feed cannot hand our cache keys to a third party.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub origin: Option<String>,
}

impl Validators {
    /// True when these validators belong to `url`'s origin.
    fn apply_to(&self, url: &Url) -> bool {
        self.origin.as_deref() == Some(url.origin().ascii_serialization().as_str())
    }
}

#[derive(Debug)]
pub enum FetchOutcome {
    NotModified,
    Body {
        bytes: Vec<u8>,
        final_url: Url,
        validators: Validators,
    },
}

#[derive(Clone, Debug)]
pub struct Fetcher {
    allow_private: bool,
    timeout: Duration,
    /// Overrides the default User-Agent. Some providers (SEC EDGAR) require a contact
    /// address in it; the value is operator-supplied configuration, never a credential.
    user_agent: Option<String>,
    /// Hosts for which private addresses are permitted, on top of the deny-list.
    /// Empty by default, so a private address is refused unless an admin names the
    /// exact host. The permission is per host, never a global switch: one internal
    /// service cannot open the door to every internal address.
    private_hosts: Vec<String>,
}

impl Fetcher {
    /// The only constructor used outside tests.
    pub fn public_only() -> Self {
        Self {
            allow_private: false,
            timeout: Duration::from_secs(25),
            user_agent: None,
            private_hosts: Vec::new(),
        }
    }

    /// Allows private addresses for exactly these hosts. Matching is on the host name
    /// as written in the URL (lowercase), never a suffix or wildcard.
    pub fn with_private_hosts(mut self, hosts: Vec<String>) -> Self {
        self.private_hosts = hosts
            .into_iter()
            .map(|host| host.trim().to_ascii_lowercase())
            .filter(|host| !host.is_empty())
            .collect();
        self
    }

    pub(crate) fn host_allows_private(&self, url: &Url) -> bool {
        let Some(host) = url.host_str() else {
            return false;
        };
        self.private_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    }

    /// Same guards, with an explicit User-Agent for providers that demand one.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let value = user_agent.into();
        let value = value.trim().to_string();
        self.user_agent = (!value.is_empty()).then_some(value);
        self
    }

    /// Tests talk to a loopback feed server; production code never builds this.
    #[cfg(test)]
    pub fn allow_loopback_for_tests() -> Self {
        Self {
            allow_private: true,
            timeout: Duration::from_secs(5),
            user_agent: None,
            private_hosts: Vec::new(),
        }
    }

    pub async fn get(
        &self,
        url: &str,
        validators: &Validators,
    ) -> Result<FetchOutcome, FetchError> {
        self.get_inner(url, validators, None).await
    }

    async fn get_inner(
        &self,
        url: &str,
        validators: &Validators,
        extra: Option<(&'static str, &str)>,
    ) -> Result<FetchOutcome, FetchError> {
        let mut current = self.target(url)?;
        let mut redirects_made = 0usize;
        for _ in 0..=MAX_REDIRECTS {
            let addrs = self.resolve(&current).await?;
            let mut builder = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .connect_timeout(Duration::from_secs(10))
                .timeout(self.timeout)
                .user_agent(self.user_agent.as_deref().unwrap_or(USER_AGENT));
            if let Some(Host::Domain(domain)) = current.host() {
                builder = builder.resolve_to_addrs(domain, &addrs);
            }
            let client = builder.build().map_err(|_| FetchError::Network)?;
            let mut request = client.get(current.clone()).header(
                ACCEPT,
                "application/rss+xml, application/atom+xml, application/rdf+xml;q=0.9, application/xml;q=0.8, text/xml;q=0.8, */*;q=0.1",
            );
            // An authentication header belongs to its origin: a redirect elsewhere must
            // not carry it.
            if let Some((name, value)) = extra
                && redirects_made == 0
            {
                request = request.header(name, value);
            }
            // Validators are replayed only to the origin that issued them. A redirect
            // to another origin must not receive our ETag/Last-Modified.
            if validators.apply_to(&current) {
                if let Some(etag) = &validators.etag {
                    request = request.header(IF_NONE_MATCH, etag);
                }
                if let Some(modified) = &validators.last_modified {
                    request = request.header(IF_MODIFIED_SINCE, modified);
                }
            }
            let mut response = request.send().await.map_err(classify)?;
            let status = response.status();
            if status == StatusCode::NOT_MODIFIED {
                return Ok(FetchOutcome::NotModified);
            }
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or(FetchError::InvalidUrl)?;
                let next = current.join(location).map_err(|_| FetchError::InvalidUrl)?;
                current = self.target(next.as_str())?;
                redirects_made += 1;
                continue;
            }
            if !status.is_success() {
                return Err(FetchError::HttpStatus(status.as_u16()));
            }
            if response
                .content_length()
                .is_some_and(|len| len > MAX_BODY_BYTES as u64)
            {
                return Err(FetchError::TooLarge);
            }
            let header = |name| {
                response
                    .headers()
                    .get(name)
                    .and_then(|v: &reqwest::header::HeaderValue| v.to_str().ok())
                    .filter(|v| v.len() <= 256)
                    .map(str::to_string)
            };
            let validators = Validators {
                etag: header(ETAG),
                last_modified: header(LAST_MODIFIED),
                origin: Some(current.origin().ascii_serialization()),
            };
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(classify)? {
                if bytes.len() + chunk.len() > MAX_BODY_BYTES {
                    return Err(FetchError::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            return Ok(FetchOutcome::Body {
                bytes,
                final_url: current,
                validators,
            });
        }
        Err(FetchError::TooManyRedirects)
    }

    /// POST a JSON body to an LLM-style endpoint, with the same guards as `get`.
    ///
    /// The optional bearer token is sent only to the first hop: a redirect to another
    /// origin must not receive it. The body is bounded before it is sent.
    pub async fn post_json(
        &self,
        url: &str,
        body: &str,
        bearer: Option<&str>,
    ) -> Result<FetchOutcome, FetchError> {
        const MAX_REQUEST_BYTES: usize = 64 * 1024;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(FetchError::TooLarge);
        }
        let mut current = self.target(url)?;
        let mut redirects_made = 0usize;
        for _ in 0..=MAX_REDIRECTS {
            let addrs = self.resolve(&current).await?;
            let mut builder = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .connect_timeout(Duration::from_secs(10))
                .timeout(self.timeout)
                .user_agent(self.user_agent.as_deref().unwrap_or(USER_AGENT));
            if let Some(Host::Domain(domain)) = current.host() {
                builder = builder.resolve_to_addrs(domain, &addrs);
            }
            let client = builder.build().map_err(|_| FetchError::Network)?;
            let mut request = client
                .post(current.clone())
                .header(CONTENT_TYPE, "application/json")
                .header(ACCEPT, "application/json");
            if let Some(token) = bearer
                && redirects_made == 0
            {
                request = request.header(AUTHORIZATION, format!("Bearer {token}"));
            }
            let mut response = request
                .body(body.to_string())
                .send()
                .await
                .map_err(classify)?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or(FetchError::InvalidUrl)?;
                let next = current.join(location).map_err(|_| FetchError::InvalidUrl)?;
                current = self.target(next.as_str())?;
                redirects_made += 1;
                continue;
            }
            if !status.is_success() {
                return Err(FetchError::HttpStatus(status.as_u16()));
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(classify)? {
                if bytes.len() + chunk.len() > MAX_BODY_BYTES {
                    return Err(FetchError::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            return Ok(FetchOutcome::Body {
                bytes,
                final_url: current,
                validators: Validators::default(),
            });
        }
        Err(FetchError::TooManyRedirects)
    }

    /// Same guards, with one extra request header. Providers that authenticate with a
    /// header (CoinGecko Demo) never put the key in the URL, so it cannot leak into a
    /// redirect target or a log line.
    pub async fn get_with_header(
        &self,
        url: &str,
        name: &'static str,
        value: &str,
    ) -> Result<FetchOutcome, FetchError> {
        self.get_inner(url, &Validators::default(), Some((name, value)))
            .await
    }

    fn target(&self, value: &str) -> Result<Url, FetchError> {
        let url = parse_target(value)?;
        let port = url.port_or_known_default().ok_or(FetchError::InvalidUrl)?;
        // A named private host may use any port (an internal Hub often runs off-standard).
        let private_ok = self.allow_private || self.host_allows_private(&url);
        if !private_ok && !ALLOWED_PORTS.contains(&port) {
            return Err(FetchError::InvalidUrl);
        }
        Ok(url)
    }

    async fn resolve(&self, url: &Url) -> Result<Vec<SocketAddr>, FetchError> {
        let port = url.port_or_known_default().ok_or(FetchError::InvalidUrl)?;
        let addrs: Vec<SocketAddr> = match url.host().ok_or(FetchError::InvalidUrl)? {
            Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
            Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
            Host::Domain(domain) => tokio::time::timeout(
                Duration::from_secs(5),
                tokio::net::lookup_host((domain, port)),
            )
            .await
            .map_err(|_| FetchError::Dns)?
            .map_err(|_| FetchError::Dns)?
            .collect(),
        };
        if addrs.is_empty() {
            return Err(FetchError::Dns);
        }
        let private_allowed = self.allow_private || self.host_allows_private(url);
        if !private_allowed && addrs.iter().any(|addr| !is_public_ip(addr.ip())) {
            return Err(FetchError::BlockedAddress);
        }
        Ok(addrs)
    }
}

fn classify(err: reqwest::Error) -> FetchError {
    if err.is_timeout() {
        FetchError::Timeout
    } else {
        FetchError::Network
    }
}

/// Syntax and policy checks that do not need the network.
pub fn parse_target(value: &str) -> Result<Url, FetchError> {
    let mut url = Url::parse(value.trim()).map_err(|_| FetchError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host().is_none()
    {
        return Err(FetchError::InvalidUrl);
    }
    url.set_fragment(None);
    Ok(url)
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(v6),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10 shared/CGNAT (also NetBird)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24 protocol assignments
        || (o[0] == 192 && o[1] == 88 && o[2] == 99) // 6to4 relay
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15 benchmarking
        || o[0] >= 240) // reserved
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    let global_unicast = (s[0] & 0xe000) == 0x2000;
    global_unicast
        && !(s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        && !(s[0] == 0x2001 && s[1] < 0x0200) // 2001::/23 IETF special (incl. Teredo)
        && s[0] != 0x2002 // 6to4 embeds IPv4
        && !(s[0] == 0x3fff && s[1] < 0x1000) // 3fff::/20 documentation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_public_addresses() {
        for ip in [
            "127.0.0.1",
            "10.20.2.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.1.2",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "198.18.0.1",
            "240.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "2001:db8::1",
            "2002:a00:1::1",
            "ff02::1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} must be blocked");
        }
        for ip in [
            "1.1.1.1",
            "93.184.216.34",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip} must be allowed");
        }
    }

    #[test]
    fn rejects_unsafe_targets_before_any_lookup() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.org/feed",
            "https://user:pw@example.org/feed",
            "gopher://example.org/",
            "not a url",
        ] {
            assert_eq!(
                parse_target(url).unwrap_err(),
                FetchError::InvalidUrl,
                "{url}"
            );
        }
        assert_eq!(
            parse_target("https://example.org/feed#x").unwrap().as_str(),
            "https://example.org/feed"
        );
    }

    #[tokio::test]
    async fn public_fetcher_blocks_loopback_and_private_literals() {
        let fetcher = Fetcher::public_only();
        assert_eq!(
            fetcher
                .get("https://example.org:22/feed", &Validators::default())
                .await
                .unwrap_err(),
            FetchError::InvalidUrl
        );
        for url in [
            "http://127.0.0.1/feed",
            "http://[::1]/feed",
            "http://10.0.0.1/feed",
            "http://localhost/feed",
        ] {
            assert_eq!(
                fetcher.get(url, &Validators::default()).await.unwrap_err(),
                FetchError::BlockedAddress,
                "{url}"
            );
        }
    }
}
