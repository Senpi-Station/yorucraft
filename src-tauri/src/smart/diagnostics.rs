use std::time::{Duration, Instant};

use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DiagnosticError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, DiagnosticError>;

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticResult {
    pub endpoint_name: String,
    pub url: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub dns_resolution_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullDiagnostics {
    pub results: Vec<DiagnosticResult>,
    pub overall_reachable: bool,
    pub recommendations: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPingResult {
    pub address: String,
    pub port: u16,
    pub online: bool,
    pub latency_ms: Option<u64>,
    pub version: Option<String>,
    pub motd: Option<String>,
    pub player_online: Option<u32>,
    pub player_max: Option<u32>,
    pub favicon: Option<String>,
}

// ─── Endpoint definitions ──────────────────────────────────────────

pub fn get_minecraft_endpoints() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Mojang Auth", "https://authserver.mojang.com"),
        ("Mojang Session", "https://sessionserver.mojang.com"),
        ("Mojang API", "https://api.mojang.com"),
        ("Minecraft Services", "https://api.minecraftservices.com"),
        ("Microsoft Auth", "https://login.live.com"),
        ("Xbox Live Auth", "https://user.auth.xboxlive.com"),
        ("XSTS Auth", "https://xsts.auth.xboxlive.com"),
        ("Asset CDN", "https://resources.download.minecraft.net"),
        ("Library CDN", "https://libraries.minecraft.net"),
        ("Maven Central", "https://repo.maven.apache.org"),
        ("Fabric Meta", "https://meta.fabricmc.net"),
        ("Modrinth API", "https://api.modrinth.com"),
    ]
}

// ─── Diagnostics ───────────────────────────────────────────────────

pub async fn run_diagnostics() -> FullDiagnostics {
    let endpoints = get_minecraft_endpoints();
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let results: Vec<DiagnosticResult> = join_all(
        endpoints.iter().map(|(name, url)| test_endpoint(&client, name, url)),
    )
    .await;

    let overall_reachable = results.iter().any(|r| r.reachable);
    let recommendations = generate_recommendations(&results);

    FullDiagnostics {
        results,
        overall_reachable,
        recommendations,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

pub async fn test_endpoint(client: &Client, name: &str, url: &str) -> DiagnosticResult {
    let start = Instant::now();

    let dns_ms = {
        let dns_start = Instant::now();
        let _ = tokio::net::lookup_host(&url[8..].split('/').next().unwrap_or("")).await;
        dns_start.elapsed().as_millis() as u64
    };

    let result = client.get(url).send().await;

    match result {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as u64;
            let status = resp.status().as_u16();
            DiagnosticResult {
                endpoint_name: name.into(),
                url: url.into(),
                reachable: status < 500,
                latency_ms: Some(latency),
                status_code: Some(status),
                error: None,
                dns_resolution_ms: Some(dns_ms),
            }
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as u64;
            DiagnosticResult {
                endpoint_name: name.into(),
                url: url.into(),
                reachable: false,
                latency_ms: Some(latency),
                status_code: None,
                error: Some(e.to_string()),
                dns_resolution_ms: Some(dns_ms),
            }
        }
    }
}

pub fn generate_recommendations(results: &[DiagnosticResult]) -> Vec<String> {
    let mut recs = Vec::new();

    let auth_down = results
        .iter()
        .filter(|r| r.endpoint_name.contains("Auth"))
        .any(|r| !r.reachable);
    if auth_down {
        recs.push("Authentication servers are down — you may not be able to log in".into());
    }

    let cdn_down = results
        .iter()
        .filter(|r| r.endpoint_name.contains("CDN"))
        .any(|r| !r.reachable);
    if cdn_down {
        recs.push("Asset download servers unreachable — game downloads may fail".into());
    }

    let slow = results.iter().filter_map(|r| r.latency_ms).any(|ms| ms > 500);
    if slow {
        recs.push("High latency detected (>500ms) — game downloads may be slow".into());
    }

    let slow_dns = results
        .iter()
        .filter_map(|r| r.dns_resolution_ms)
        .any(|ms| ms > 200);
    if slow_dns {
        recs.push("DNS resolution is slow — consider using 1.1.1.1 or 8.8.8.8".into());
    }

    if recs.is_empty() {
        recs.push("All endpoints reachable — network looks good".into());
    }

    recs
}

// ─── Public IP ─────────────────────────────────────────────────────

pub async fn get_public_ip(client: &Client) -> Result<String> {
    let resp = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await?;
    let v: serde_json::Value = resp.json().await?;
    let ip = v["ip"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    Ok(ip)
}

// ─── Server ping ───────────────────────────────────────────────────

fn offline_result(address: &str, port: u16) -> ServerPingResult {
    ServerPingResult {
        address: address.into(),
        port,
        online: false,
        latency_ms: None,
        version: None,
        motd: None,
        player_online: None,
        player_max: None,
        favicon: None,
    }
}

pub async fn test_server(address: &str, port: u16) -> Result<ServerPingResult> {
    let addr = format!("{}:{}", address, port);
    let timeout = Duration::from_secs(3);

    let start = Instant::now();
    match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => Ok(ServerPingResult {
            address: address.into(),
            port,
            online: true,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            version: None,
            motd: None,
            player_online: None,
            player_max: None,
            favicon: None,
        }),
        _ => Ok(offline_result(address, port)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoints_not_empty() {
        let endpoints = get_minecraft_endpoints();
        assert!(endpoints.len() >= 10);
    }

    #[test]
    fn test_recommendations_empty_results() {
        let recs = generate_recommendations(&[]);
        assert!(recs.iter().any(|r| r.contains("good")));
    }

    #[test]
    fn test_recommendations_auth_down() {
        let results = vec![DiagnosticResult {
            endpoint_name: "Mojang Auth".into(),
            url: "https://authserver.mojang.com".into(),
            reachable: false,
            latency_ms: None,
            status_code: None,
            error: Some("timeout".into()),
            dns_resolution_ms: None,
        }];
        let recs = generate_recommendations(&results);
        assert!(recs.iter().any(|r| r.contains("Authentication")));
    }

    #[test]
    fn test_recommendations_slow() {
        let results = vec![DiagnosticResult {
            endpoint_name: "Asset CDN".into(),
            url: "https://resources.download.minecraft.net".into(),
            reachable: true,
            latency_ms: Some(600),
            status_code: Some(200),
            error: None,
            dns_resolution_ms: None,
        }];
        let recs = generate_recommendations(&results);
        assert!(recs.iter().any(|r| r.contains("latency")));
    }

    #[tokio::test]
    async fn test_server_offline() {
        let result = test_server("192.0.2.1", 25565).await.unwrap();
        assert!(!result.online);
    }
}
