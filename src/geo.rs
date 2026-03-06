use serde::Deserialize;
use tracing::{debug, info};

#[derive(Deserialize)]
struct GeoResponse {
    #[serde(rename = "countryCode")]
    country_code: String,
}

/// Look up the ISO 3166-1 alpha-2 country code for the given IP address.
/// Returns `None` for private/loopback addresses or on any error.
pub async fn lookup_country(ip: &str) -> Option<String> {
    if is_private_ip(ip) {
        debug!(ip, "skipping geolocation for private/loopback IP");
        return None;
    }

    let url = format!("http://ip-api.com/json/{}?fields=countryCode", ip);
    let response = match reqwest::get(&url).await {
        Ok(r) => r,
        Err(e) => {
            debug!(ip, error = %e, "geolocation request failed");
            return None;
        }
    };

    match response.json::<GeoResponse>().await {
        Ok(geo) if !geo.country_code.is_empty() => {
            info!(ip, country = %geo.country_code, "resolved player country");
            Some(geo.country_code)
        }
        Ok(_) => {
            debug!(ip, "empty country code in geolocation response");
            None
        }
        Err(e) => {
            debug!(ip, error = %e, "failed to parse geolocation response");
            None
        }
    }
}

fn is_private_ip(ip: &str) -> bool {
    match ip.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        Ok(std::net::IpAddr::V6(v6)) => v6.is_loopback(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loopback_ipv4_returns_none() {
        assert_eq!(lookup_country("127.0.0.1").await, None);
    }

    #[tokio::test]
    async fn private_ip_10_returns_none() {
        assert_eq!(lookup_country("10.0.0.1").await, None);
    }

    #[tokio::test]
    async fn private_ip_192_168_returns_none() {
        assert_eq!(lookup_country("192.168.1.1").await, None);
    }

    #[tokio::test]
    async fn ipv6_loopback_returns_none() {
        assert_eq!(lookup_country("::1").await, None);
    }
}
