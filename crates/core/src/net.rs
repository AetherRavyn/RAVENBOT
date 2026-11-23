//! Shared checks for URLs supplied by users or models.
//!
//! These guards are deliberately narrow: normal public URLs, localhost, and
//! private LAN services still work because local-first integrations are core to
//! RAVENBOT. Non-HTTP schemes, embedded credentials, cloud metadata hosts, and
//! link-local/unspecified addresses are refused.

use std::net::IpAddr;

use url::{Host, Url};

const METADATA_HOSTS: &[&str] = &[
    "metadata.google.internal",
    "metadata",
    "instance-data",
    "instance-data.ec2.internal",
];

/// Whether a hostname is an explicit local/private name. Literal IPs are
/// checked separately with [`is_private_ip`].
pub fn is_private_host(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".lan")
        || host.ends_with(".home.arpa")
}

/// Loopback, private, link-local, and unique-local addresses.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || ip.is_documentation()
        }
        IpAddr::V6(ip) => ip.is_unicast_link_local() || ip.is_unspecified(),
    }
}

/// Parse and validate a user-supplied HTTP URL.
pub fn validate_http_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|error| format!("Invalid URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only http and https URLs are allowed.".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URLs with embedded credentials are not allowed.".to_string());
    }
    let host = url
        .host_str()
        .filter(|host| !host.trim().is_empty())
        .ok_or_else(|| "The URL has no host.".to_string())?;
    if METADATA_HOSTS
        .iter()
        .any(|blocked| host.eq_ignore_ascii_case(blocked))
    {
        return Err("Cloud metadata endpoints are not reachable through this tool.".to_string());
    }
    if let Some(ip) = url.host().and_then(|host| match host {
        Host::Ipv4(ip) => Some(IpAddr::V4(ip)),
        Host::Ipv6(ip) => Some(IpAddr::V6(ip)),
        Host::Domain(_) => None,
    }) {
        if is_blocked_ip(ip) {
            return Err(
                "Link-local, unspecified, and documentation addresses are blocked.".to_string(),
            );
        }
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_normal_public_and_local_urls() {
        assert!(validate_http_url("https://example.com/path?q=1").is_ok());
        assert!(validate_http_url("http://localhost:8080/api").is_ok());
        assert!(validate_http_url("http://192.168.1.10:3000/").is_ok());
    }

    #[test]
    fn rejects_non_http_schemes_and_credentials() {
        assert!(validate_http_url("file:///etc/passwd").is_err());
        assert!(validate_http_url("ftp://example.com/file").is_err());
        assert!(validate_http_url("https://user:pass@example.com/").is_err());
    }

    #[test]
    fn rejects_cloud_metadata_and_link_local_addresses() {
        assert!(validate_http_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_http_url("http://metadata.google.internal/computeMetadata/v1").is_err());
        assert!(validate_http_url("http://[fe80::1]/").is_err());
        assert!(validate_http_url("http://0.0.0.0/").is_err());
    }
}
