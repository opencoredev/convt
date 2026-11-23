//! Outbound requests to URLs that customers choose: `source_url` downloads in the
//! worker and webhook deliveries from the API. Both run next to database and
//! storage credentials, so a URL may only reach public internet addresses.
//!
//! [`check`] validates the URL's shape when a job is created. [`client_for`]
//! resolves the host, refuses the request unless every address is public, and
//! pins the connection to the address it checked, so a DNS answer that changes
//! between the check and the connect cannot reach a private network. Redirects
//! are never followed.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::Url;

pub const MAX_URL_LEN: usize = 2048;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UrlError {
    #[error("must be an absolute https URL of at most 2048 characters")]
    Invalid,
    #[error("must not carry a user name or password")]
    Credentials,
    #[error("must point at a public internet host")]
    PrivateHost,
    #[error("host did not resolve")]
    Unresolved,
}

/// Whether private addresses are allowed. Only tests and local development turn
/// this on (`CONVT_ALLOW_PRIVATE_URLS=1`); production never does.
#[derive(Clone, Copy, Debug, Default)]
pub struct Policy {
    pub allow_private: bool,
}

impl Policy {
    pub fn from_env() -> Self {
        Self {
            allow_private: std::env::var("CONVT_ALLOW_PRIVATE_URLS").as_deref() == Ok("1"),
        }
    }
}

/// Parses and checks a URL without resolving it.
pub fn check(raw: &str, policy: Policy) -> Result<Url, UrlError> {
    if raw.len() > MAX_URL_LEN {
        return Err(UrlError::Invalid);
    }
    let url = Url::parse(raw).map_err(|_| UrlError::Invalid)?;
    let scheme_ok = url.scheme() == "https" || (policy.allow_private && url.scheme() == "http");
    if !scheme_ok || url.host().is_none() || url.cannot_be_a_base() {
        return Err(UrlError::Invalid);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UrlError::Credentials);
    }
    if policy.allow_private {
        return Ok(url);
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) if !is_public(IpAddr::V4(ip)) => Err(UrlError::PrivateHost),
        Some(url::Host::Ipv6(ip)) if !is_public(IpAddr::V6(ip)) => Err(UrlError::PrivateHost),
        Some(url::Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            if name == "localhost"
                || name.ends_with(".localhost")
                || name.ends_with(".internal")
                || name.ends_with(".local")
                || !name.contains('.')
            {
                Err(UrlError::PrivateHost)
            } else {
                Ok(url)
            }
        }
        _ => Ok(url),
    }
}

/// A client whose only connection goes to a checked public address of the URL's
/// host. Build one per request.
pub async fn client_for(
    url: &Url,
    policy: Policy,
    timeout: Duration,
) -> Result<reqwest::Client, UrlError> {
    let host = url.host_str().ok_or(UrlError::Invalid)?;
    let port = url.port_or_known_default().ok_or(UrlError::Invalid)?;
    let builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .user_agent("convt-cloud/1 (+https://convt.app/docs)")
        .no_proxy();
    let lookup = host.trim_start_matches('[').trim_end_matches(']');
    let addresses: Vec<SocketAddr> = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::lookup_host((lookup, port)),
    )
    .await
    .map_err(|_| UrlError::Unresolved)?
    .map_err(|_| UrlError::Unresolved)?
    .collect();
    let Some(first) = addresses.first().copied() else {
        return Err(UrlError::Unresolved);
    };
    if !policy.allow_private && addresses.iter().any(|a| !is_public(a.ip())) {
        return Err(UrlError::PrivateHost);
    }
    builder
        .resolve(lookup, first)
        .build()
        .map_err(|_| UrlError::Invalid)
}

/// Globally routable unicast only. Everything a cloud network uses internally
/// (RFC 1918, CGNAT, link-local metadata, unique-local IPv6, NAT64 and mapped
/// forms of those) is refused.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            is_public_v6(v6)
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    // Only 2000::/3 is global unicast; inside it, refuse documentation,
    // 6to4/Teredo wrappers that can carry private IPv4, and NAT64.
    (s[0] & 0xe000) == 0x2000
        && !(s[0] == 0x2001 && s[1] == 0x0db8)
        && !(s[0] == 0x2001 && s[1] == 0x0000)
        && s[0] != 0x2002
        && !(s[0] == 0x0064 && s[1] == 0xff9b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRICT: Policy = Policy {
        allow_private: false,
    };

    #[test]
    fn refuses_private_and_odd_urls() {
        for bad in [
            "http://example.com/a.png",
            "ftp://example.com/a.png",
            "https://user:pw@example.com/a.png",
            "https://localhost/a.png",
            "https://api.localhost/a.png",
            "https://postgres.railway.internal/a",
            "https://printer.local/a",
            "https://intranet/a",
            "https://127.0.0.1/a",
            "https://10.0.0.8/a",
            "https://172.16.0.1/a",
            "https://192.168.1.1/a",
            "https://169.254.169.254/latest/meta-data",
            "https://100.64.0.1/a",
            "https://0.0.0.0/a",
            "https://[::1]/a",
            "https://[fd12:3456::1]/a",
            "https://[fe80::1]/a",
            "https://[::ffff:10.0.0.1]/a",
            "https://[64:ff9b::a00:1]/a",
            "not a url",
        ] {
            assert!(check(bad, STRICT).is_err(), "{bad} should be refused");
        }
        assert!(check(&format!("https://example.com/{}", "a".repeat(2100)), STRICT).is_err());
    }

    #[test]
    fn accepts_public_https() {
        for good in [
            "https://example.com/a.png",
            "https://bucket.s3.amazonaws.com/a.png?X-Amz-Signature=abc",
            "https://93.184.216.34/a.png",
            "https://[2606:4700::1111]/a",
        ] {
            assert!(check(good, STRICT).is_ok(), "{good} should be accepted");
        }
    }

    #[test]
    fn address_classes() {
        assert!(is_public("8.8.8.8".parse().unwrap()));
        assert!(!is_public("198.18.0.1".parse().unwrap()));
        assert!(!is_public("240.0.0.1".parse().unwrap()));
        assert!(!is_public("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_public("2002:0a00:0001::1".parse().unwrap()));
        assert!(is_public("2606:4700:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    async fn resolution_refuses_names_that_point_inside() {
        // `localhost` passes nothing in strict mode even when it resolves.
        let url = Url::parse("https://localhost/a").unwrap();
        assert!(
            client_for(&url, STRICT, Duration::from_secs(1))
                .await
                .is_err()
        );
    }
}
