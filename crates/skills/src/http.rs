//! Shared HTTP client for network-backed tools.
//!
//! A total request timeout is wrong for large downloads and streamed
//! responses, so tools get a connect timeout and an idle read timeout. A
//! stalled socket fails; a slow but active response continues.

use std::time::Duration;

fn env_secs(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub fn http_client() -> reqwest::Client {
    http_client_with_timeouts(
        Duration::from_secs(env_secs("RAVENBOT_TOOL_CONNECT_TIMEOUT_SECS", 10)),
        Duration::from_secs(env_secs("RAVENBOT_TOOL_READ_TIMEOUT_SECS", 120)),
    )
}

/// Build a client whose DNS answer is pinned to the first validated address.
/// This closes the window where a hostname passes validation and then resolves
/// somewhere else for the actual connection.
pub async fn pinned_client(raw: &str) -> Result<(reqwest::Client, reqwest::Url), String> {
    let url = ravenbot_core::net::validate_http_url(raw)?;
    let host = url
        .host_str()
        .ok_or_else(|| "The URL has no host.".to_string())?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "The URL has no port.".to_string())?;
    let addresses: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|error| format!("Could not resolve {host}: {error}"))?
        .collect();
    if addresses.is_empty() {
        return Err(format!("{host} resolved to no addresses."));
    }
    let literal = url
        .host()
        .is_some_and(|host| matches!(host, url::Host::Ipv4(_) | url::Host::Ipv6(_)));
    let private_name = host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".lan")
        || host.ends_with(".home.arpa");
    for address in &addresses {
        let ip = address.ip();
        if ravenbot_core::net::is_blocked_ip(ip) {
            return Err(format!("{host} resolved to a blocked address ({ip})."));
        }
        if !literal && !private_name && ravenbot_core::net::is_private_ip(ip) {
            return Err(format!(
                "{host} resolved to a private address ({ip}); use a private hostname or an explicit local URL."
            ));
        }
    }
    let address = addresses[0];
    let client = reqwest::Client::builder()
        .user_agent("ravenbot/0.2.0 (https://github.com/AetherRavyn/RAVENBOT)")
        .connect_timeout(Duration::from_secs(env_secs(
            "RAVENBOT_TOOL_CONNECT_TIMEOUT_SECS",
            10,
        )))
        .read_timeout(Duration::from_secs(env_secs(
            "RAVENBOT_TOOL_READ_TIMEOUT_SECS",
            120,
        )))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            match ravenbot_core::net::validate_http_url(attempt.url().as_str()) {
                Ok(_) => attempt.follow(),
                Err(error) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    error,
                )),
            }
        }))
        .resolve(&host, address)
        .build()
        .map_err(|error| error.to_string())?;
    Ok((client, url))
}

pub fn http_client_with_timeouts(connect: Duration, read: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("ravenbot/0.2.0 (https://github.com/AetherRavyn/RAVENBOT)")
        .connect_timeout(connect)
        .read_timeout(read)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            match ravenbot_core::net::validate_http_url(attempt.url().as_str()) {
                Ok(_) => attempt.follow(),
                Err(error) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    error,
                )),
            }
        }))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn pinned_client_connects_and_refuses_metadata_before_resolution() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut request = [0u8; 2048];
            let _ = socket.read(&mut request).await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await;
        });
        let (client, _url) = pinned_client(&format!("http://127.0.0.1:{}/", address.port()))
            .await
            .expect("literal loopback is allowed");
        let response = client
            .get(format!("http://127.0.0.1:{}/", address.port()))
            .send()
            .await;
        assert!(response.is_ok(), "pinned client request: {response:?}");
        server.abort();

        let error = pinned_client("http://169.254.169.254/latest/meta-data")
            .await
            .expect_err("metadata is refused");
        assert!(error.contains("blocked") || error.contains("Link-local"));
    }

    #[tokio::test]
    async fn a_stalled_tool_connection_hits_the_read_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            // Accept and stay silent, holding the socket open.
            let (_socket, _) = listener.accept().await.expect("accept");
            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        let client = http_client_with_timeouts(Duration::from_secs(2), Duration::from_millis(300));
        let started = Instant::now();
        let error = client
            .get(format!("http://{address}/"))
            .send()
            .await
            .expect_err("a stalled response must fail");
        assert!(error.is_timeout(), "expected a timeout, got {error:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
        server.abort();
    }
}
