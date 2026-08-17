use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FabricError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Fabric does not support Minecraft {0}")]
    UnsupportedVersion(String),
}

pub type Result<T> = std::result::Result<T, FabricError>;

const META_BASE: &str = "https://meta.fabricmc.net/v2";

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    #[serde(rename = "type")]
    pub profile_type: String,
    #[serde(default)]
    pub classifiers: std::collections::HashMap<String, ArtifactInfo>,
    #[serde(default)]
    pub depends: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricVersionEntry {
    pub loader: FabricLoaderInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricLoaderInfo {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricInstallerVersion {
    pub version: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricVersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    pub main_class: Option<String>,
    pub arguments: Option<serde_json::Value>,
    pub libraries: Vec<serde_json::Value>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// ─── API: Fetch loader info ────────────────────────────────────────

pub async fn get_fabric_loader(
    client: &reqwest::Client,
    version_id: &str,
) -> Result<FabricProfile> {
    let url = format!("{}/versions/loader/{}", META_BASE, version_id);
    let resp = client.get(&url).send().await?;

    if resp.status().as_u16() == 404 {
        return Err(FabricError::UnsupportedVersion(version_id.to_string()));
    }

    let profiles: Vec<FabricProfile> = resp.json().await?;
    profiles
        .into_iter()
        .next()
        .ok_or_else(|| FabricError::UnsupportedVersion(version_id.to_string()))
}

pub async fn get_fabric_installer(client: &reqwest::Client) -> Result<FabricInstallerVersion> {
    let url = format!("{}/versions/installer", META_BASE);
    let resp = client.get(&url).send().await?;
    let versions: Vec<FabricInstallerVersion> = resp.json().await?;
    versions
        .into_iter()
        .next()
        .ok_or_else(|| FabricError::UnsupportedVersion("no installer found".into()))
}

// ─── API: Supported game versions ──────────────────────────────────

pub async fn get_fabric_versions(client: &reqwest::Client) -> Result<Vec<String>> {
    let url = format!("{}/versions/game", META_BASE);
    let resp = client.get(&url).send().await?;
    let entries: Vec<serde_json::Value> = resp.json().await?;

    let versions: Vec<String> = entries
        .iter()
        .filter(|v| v["stable"].as_bool().unwrap_or(false))
        .filter_map(|v| v["version"].as_str().map(String::from))
        .collect();

    Ok(versions)
}

// ─── Installation ──────────────────────────────────────────────────

pub async fn install_fabric(
    version_id: &str,
    game_dir: &Path,
    progress_fn: impl Fn(u64, u64),
) -> Result<PathBuf> {
    let client = reqwest::Client::new();
    let profile = get_fabric_loader(&client, version_id).await?;
    let installer = get_fabric_installer(&client).await?;

    let version_dir = game_dir
        .join("versions")
        .join(format!("{}-fabric", version_id));
    std::fs::create_dir_all(&version_dir)?;

    let libraries_dir = game_dir.join("libraries");
    std::fs::create_dir_all(&libraries_dir)?;

    // ── Download installer JAR to extract profile metadata ──
    let installer_jar = download_artifact(
        &client,
        &installer.url,
        &installer.sha1,
        &version_dir.join("installer.jar"),
    )
    .await?;

    // ── Extract the version JSON from installer JAR ──
    let version_json = extract_profile_from_installer(&installer_jar, version_id, &profile)?;

    // ── Download all libraries ──
    let mut all_libraries: Vec<serde_json::Value> = Vec::new();
    let total_bytes: u64 = profile
        .classifiers
        .values()
        .map(|a| a.size)
        .sum();
    let mut downloaded: u64 = 0;

    // Download classifier artifacts (loader jars, mappings)
    for (_platform, artifact) in &profile.classifiers {
        download_library(&client, artifact, &libraries_dir).await?;
        downloaded += artifact.size;
        progress_fn(downloaded, total_bytes);
    }

    // Add libraries from the version JSON
    if !version_json.libraries.is_empty() {
        all_libraries.extend(version_json.libraries.iter().cloned());
    }

    // ── Build final version JSON ──
    let mut json = serde_json::Map::new();
    json.insert("id".into(), serde_json::Value::String(format!("{}-fabric", version_id)));
    json.insert("inheritsFrom".into(), serde_json::Value::String(version_id.to_string()));
    json.insert(
        "mainClass".into(),
        serde_json::Value::String(
            profile
                .depends
                .get("fabricloader")
                .cloned()
                .unwrap_or_else(|| "net.fabricmc.loader.impl.launch.knot.KnotClient".into()),
        ),
    );
    json.insert("libraries".into(), serde_json::Value::Array(all_libraries));

    let json_path = version_dir.join(format!("{}-fabric.json", version_id));
    std::fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;

    // Clean up installer JAR
    let _ = std::fs::remove_file(installer_jar);

    Ok(version_dir)
}

// ─── Helpers ───────────────────────────────────────────────────────

fn extract_profile_from_installer(
    installer_jar: &Path,
    version_id: &str,
    profile: &FabricProfile,
) -> Result<FabricVersionJson> {
    let file = std::fs::File::open(installer_jar)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // The installer JAR contains a version JSON, usually at "profile.json" or similar
    let mut json_content = String::new();

    // Try common locations
    for name in &["profile.json", "fabric-loader-profile.json", "version.json"] {
        if let Ok(mut f) = archive.by_name(name) {
            use std::io::Read;
            f.read_to_string(&mut json_content)?;
            break;
        }
    }

    if json_content.is_empty() {
        // Build a minimal version JSON from what we know
        let loader_version = profile.depends.get("fabricloader").map(|s| s.as_str()).unwrap_or("0.15.0");
        let intermediary_version = profile.depends.get("intermediary").map(|s| s.as_str()).unwrap_or(version_id);

        return Ok(FabricVersionJson {
            id: format!("{}-fabric", version_id),
            inherits_from: Some(version_id.to_string()),
            main_class: Some("net.fabricmc.loader.impl.launch.knot.KnotClient".into()),
            arguments: None,
            libraries: vec![
                serde_json::json!({
                    "name": format!("net.fabricmc:fabric-loader:{}", loader_version),
                    "url": "https://maven.fabricmc.net/",
                }),
                serde_json::json!({
                    "name": format!("net.fabricmc:intermediary:{}", intermediary_version),
                    "url": "https://maven.fabricmc.net/",
                    "serveronly": false,
                }),
            ],
            extra: std::collections::HashMap::new(),
        });
    }

    let mut parsed: serde_json::Value = serde_json::from_str(&json_content)?;

    // Ensure id and inheritsFrom are set correctly
    parsed["id"] = serde_json::Value::String(format!("{}-fabric", version_id));
    parsed["inheritsFrom"] = serde_json::Value::String(version_id.to_string());

    Ok(serde_json::from_value(parsed)?)
}

async fn download_artifact(
    client: &reqwest::Client,
    url: &str,
    _sha1: &str,
    dest: &Path,
) -> Result<PathBuf> {
    let resp = client.get(url).send().await?;
    let bytes = resp.bytes().await?;
    std::fs::write(dest, &bytes)?;
    Ok(dest.to_path_buf())
}

async fn download_library(
    client: &reqwest::Client,
    artifact: &ArtifactInfo,
    libraries_dir: &Path,
) -> Result<PathBuf> {
    let dest = libraries_dir.join(&artifact.path);
    if dest.exists() {
        return Ok(dest);
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    download_artifact(client, &artifact.url, &artifact.sha1, &dest).await
}

pub fn is_fabric_installed(version_id: &str, game_dir: &Path) -> bool {
    let version_dir = game_dir
        .join("versions")
        .join(format!("{}-fabric", version_id));
    let json_path = version_dir.join(format!("{}-fabric.json", version_id));
    json_path.exists()
}

pub fn uninstall_fabric(version_id: &str, game_dir: &Path) -> Result<()> {
    let version_dir = game_dir
        .join("versions")
        .join(format!("{}-fabric", version_id));
    if version_dir.exists() {
        std::fs::remove_dir_all(&version_dir)?;
    }
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uninstall_fabric() {
        let dir = tempfile::tempdir().unwrap();
        let version_dir = dir
            .path()
            .join("versions")
            .join("1.20.4-fabric");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(version_dir.join("1.20.4-fabric.json"), "{}").unwrap();

        assert!(is_fabric_installed("1.20.4", dir.path()));
        uninstall_fabric("1.20.4", dir.path()).unwrap();
        assert!(!is_fabric_installed("1.20.4", dir.path()));
    }

    #[test]
    fn test_fabric_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_fabric_installed("1.20.4", dir.path()));
    }

    #[tokio::test]
    async fn test_get_fabric_versions() {
        let client = reqwest::Client::new();
        if let Ok(versions) = get_fabric_versions(&client).await {
            assert!(!versions.is_empty());
        }
    }
}
