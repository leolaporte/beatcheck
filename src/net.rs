//! Network safety helpers: SSRF guarding and bounded body reads.
//!
//! Feeds and the pages they link to are attacker-influenced. Before fetching a
//! feed-supplied URL we reject non-HTTP(S) schemes and any host that points at a
//! private, loopback, or link-local address, so a malicious feed can't make
//! BeatCheck reach into the local network or cloud metadata endpoints.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use url::{Host, Url};

/// Maximum number of bytes we will read from a single HTTP response body.
/// Guards against a hostile server streaming a huge body to exhaust memory.
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024; // 16 MiB

/// Returns true if `ip` is not a routable public address and therefore must not
/// be fetched (loopback, private, link-local, CGNAT, multicast, etc.).
pub fn ip_is_disallowed(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => ipv4_is_disallowed(v4),
        IpAddr::V6(v6) => ipv6_is_disallowed(v6),
    }
}

fn ipv4_is_disallowed(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        // 100.64.0.0/10 — carrier-grade NAT (RFC 6598)
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 0x40)
}

fn ipv6_is_disallowed(ip: Ipv6Addr) -> bool {
    // IPv4-mapped (::ffff:a.b.c.d) — unwrap and judge by the embedded v4 address.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return ipv4_is_disallowed(v4);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // fc00::/7 — unique local addresses
        || (ip.octets()[0] & 0xfe) == 0xfc
        // fe80::/10 — link-local unicast
        || (ip.octets()[0] == 0xfe && (ip.octets()[1] & 0xc0) == 0x80)
}

/// Reject a URL before fetching it if it is not safe to request from an
/// attacker-influenced source: non-HTTP(S) schemes, and hosts that point at a
/// private/loopback/link-local address (whether given as an IP literal or via a
/// hostname that resolves to one).
///
/// Note: this resolves DNS itself, and the HTTP client resolves again at connect
/// time, so a TOCTOU DNS-rebinding window technically remains. It still blocks the
/// realistic cases — feed entries pointing straight at internal addresses, and
/// hostnames statically mapped to them.
pub fn validate_public_url(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("disallowed URL scheme: {other}")),
    }

    match url.host() {
        Some(Host::Ipv4(ip)) => reject_if_private(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => reject_if_private(IpAddr::V6(ip)),
        Some(Host::Domain(domain)) => {
            if domain.eq_ignore_ascii_case("localhost") {
                return Err("refusing to fetch localhost".into());
            }
            let port = url.port_or_known_default().unwrap_or(443);
            let mut resolved = false;
            for addr in (domain, port)
                .to_socket_addrs()
                .map_err(|e| format!("DNS resolution failed for {domain}: {e}"))?
            {
                resolved = true;
                reject_if_private(addr.ip())?;
            }
            if !resolved {
                return Err(format!("host {domain} did not resolve"));
            }
            Ok(())
        }
        None => Err("URL has no host".into()),
    }
}

/// Read an HTTP response body, aborting if it exceeds `max` bytes. Streaming the
/// body chunk-by-chunk means a hostile server can't exhaust memory by sending a
/// multi-gigabyte response within the request timeout.
pub async fn read_body_capped(
    mut resp: reqwest::Response,
    max: usize,
) -> std::result::Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if buf.len() + chunk.len() > max {
            return Err(format!("response body exceeded {max}-byte cap"));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn reject_if_private(ip: IpAddr) -> Result<(), String> {
    if ip_is_disallowed(ip) {
        Err(format!("refusing to fetch private/loopback address: {ip}"))
    } else {
        Ok(())
    }
}

/// Synchronous, DNS-free check for a redirect hop. Rejects non-HTTP(S) schemes,
/// localhost, and IP-literal hosts in private ranges so a public URL can't 30x
/// us onto an internal address. Hostname hops are allowed (we can't safely
/// resolve DNS inside the redirect policy); the initial request is already
/// DNS-validated by [`validate_public_url`].
pub fn redirect_target_allowed(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => {}
        _ => return false,
    }
    match url.host() {
        Some(Host::Ipv4(ip)) => !ip_is_disallowed(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => !ip_is_disallowed(IpAddr::V6(ip)),
        Some(Host::Domain(d)) => !d.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn validate_rejects_non_http_schemes() {
        assert!(validate_public_url(&url("file:///etc/passwd")).is_err());
        assert!(validate_public_url(&url("ftp://example.com/x")).is_err());
        assert!(validate_public_url(&url("gopher://example.com/")).is_err());
    }

    #[test]
    fn validate_rejects_ip_literal_internal_hosts() {
        assert!(validate_public_url(&url("http://127.0.0.1/admin")).is_err());
        assert!(validate_public_url(&url("http://192.168.1.1/")).is_err());
        assert!(validate_public_url(&url("http://169.254.169.254/latest/meta-data/")).is_err());
        assert!(validate_public_url(&url("http://[::1]:8080/")).is_err());
    }

    #[test]
    fn validate_rejects_localhost() {
        assert!(validate_public_url(&url("http://localhost:9200/")).is_err());
        assert!(validate_public_url(&url("http://LOCALHOST/")).is_err());
    }

    #[test]
    fn validate_allows_public_ip_literal() {
        // Public IP literal needs no DNS, so this is network-independent.
        assert!(validate_public_url(&url("https://1.1.1.1/")).is_ok());
    }

    #[test]
    fn redirect_hop_blocks_internal_and_bad_schemes() {
        assert!(!redirect_target_allowed(&url("http://127.0.0.1/")));
        assert!(!redirect_target_allowed(&url("http://169.254.169.254/")));
        assert!(!redirect_target_allowed(&url("http://localhost/")));
        assert!(!redirect_target_allowed(&url("file:///etc/passwd")));
        assert!(redirect_target_allowed(&url("https://example.com/article")));
    }

    #[tokio::test]
    async fn read_body_capped_rejects_oversize_body() {
        let resp = reqwest::Response::from(http::Response::new(vec![0u8; 100]));
        let result = read_body_capped(resp, 50).await;
        assert!(result.is_err(), "body over the cap must be rejected");
    }

    #[tokio::test]
    async fn read_body_capped_returns_body_within_cap() {
        let resp = reqwest::Response::from(http::Response::new(vec![7u8; 30]));
        let out = read_body_capped(resp, 50).await.unwrap();
        assert_eq!(out, vec![7u8; 30]);
    }

    #[test]
    fn rejects_ipv4_loopback() {
        assert!(ip_is_disallowed(ip("127.0.0.1")));
    }

    #[test]
    fn rejects_ipv4_private_ranges() {
        assert!(ip_is_disallowed(ip("10.0.0.5")));
        assert!(ip_is_disallowed(ip("172.16.4.4")));
        assert!(ip_is_disallowed(ip("192.168.1.1")));
    }

    #[test]
    fn rejects_ipv4_link_local_metadata() {
        // 169.254.169.254 is the classic cloud metadata endpoint.
        assert!(ip_is_disallowed(ip("169.254.169.254")));
    }

    #[test]
    fn rejects_ipv4_cgnat_and_unspecified() {
        assert!(ip_is_disallowed(ip("100.64.0.1")));
        assert!(ip_is_disallowed(ip("0.0.0.0")));
    }

    #[test]
    fn allows_public_ipv4() {
        assert!(!ip_is_disallowed(ip("1.1.1.1")));
        assert!(!ip_is_disallowed(ip("93.184.216.34"))); // example.com
        assert!(!ip_is_disallowed(ip("8.8.8.8")));
    }

    #[test]
    fn rejects_ipv6_loopback_and_ula_and_link_local() {
        assert!(ip_is_disallowed(ip("::1")));
        assert!(ip_is_disallowed(ip("fc00::1")));
        assert!(ip_is_disallowed(ip("fd12:3456::1")));
        assert!(ip_is_disallowed(ip("fe80::1")));
    }

    #[test]
    fn rejects_ipv4_mapped_ipv6_loopback() {
        // ::ffff:127.0.0.1 must be judged as loopback, not allowed.
        assert!(ip_is_disallowed(ip("::ffff:127.0.0.1")));
    }

    #[test]
    fn allows_public_ipv6() {
        assert!(!ip_is_disallowed(ip("2606:4700:4700::1111"))); // cloudflare
    }
}
