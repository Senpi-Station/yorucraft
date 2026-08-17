use std::collections::{HashMap, HashSet};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ManifestError {
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Failed to deserialize manifest: {0}")]
    Deserialize(#[from] serde_json::Error),
    #[error("HTTP error: status {status}")]
    HttpError { status: u16 },
    #[error("Circular inheritance detected for version: {0}")]
    CircularInheritance(String),
    #[error("Version not found: {0}")]
    VersionNotFound(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

pub type Result<T> = std::result::Result<T, ManifestError>;

// ─── Top-level manifest ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    pub url: String,
    pub sha1: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub release_time: String,
    #[serde(default)]
    pub compliance_level: Option<i32>,
}

// ─── Version data ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionData {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub main_class: String,
    #[serde(default, rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndex>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub logging: Option<serde_json::Value>,
    #[serde(default)]
    pub jar: Option<String>,
    #[serde(default, rename = "minimumLauncherVersion")]
    pub minimum_launcher_version: Option<i32>,
}

// ─── Arguments ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Option<Vec<ArgValue>>,
    #[serde(default)]
    pub jvm: Option<Vec<ArgValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    String(String),
    Rules {
        rules: Vec<Rule>,
        value: ArgValueInner,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValueInner {
    String(String),
    Array(Vec<String>),
}

impl ArgValueInner {
    pub fn as_slice(&self) -> Vec<String> {
        match self {
            ArgValueInner::String(s) => vec![s.clone()],
            ArgValueInner::Array(a) => a.clone(),
        }
    }
}

// ─── Rules ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

// ─── Asset index ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

// ─── Downloads ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Downloads {
    #[serde(default)]
    pub client: Option<DownloadInfo>,
    #[serde(default)]
    pub server: Option<DownloadInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

// ─── Libraries ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub extract: Option<ExtractRule>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub md5: Option<String>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, LibraryDownloads>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractRule {
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
}

// ─── Functions ─────────────────────────────────────────────────────

pub async fn fetch_manifest(client: &Client) -> Result<VersionManifest> {
    let url = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(ManifestError::HttpError {
            status: resp.status().as_u16(),
        });
    }
    let manifest: VersionManifest = resp.json().await?;
    Ok(manifest)
}

pub async fn fetch_version_data(client: &Client, url: &str) -> Result<VersionData> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(ManifestError::HttpError {
            status: resp.status().as_u16(),
        });
    }
    let data: VersionData = resp.json().await?;
    Ok(data)
}

pub async fn resolve_version(client: &Client, version_id: &str) -> Result<VersionData> {
    let manifest = fetch_manifest(client).await?;

    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| ManifestError::VersionNotFound(version_id.to_string()))?;

    let mut data = fetch_version_data(client, &entry.url).await?;
    let mut seen = HashSet::new();
    seen.insert(data.id.clone());

    while let Some(ref parent_id) = data.inherits_from {
        if seen.contains(parent_id) {
            return Err(ManifestError::CircularInheritance(parent_id.clone()));
        }
        seen.insert(parent_id.clone());

        let parent_entry = manifest
            .versions
            .iter()
            .find(|v| v.id == *parent_id)
            .ok_or_else(|| ManifestError::VersionNotFound(parent_id.clone()))?;

        let parent_data = fetch_version_data(client, &parent_entry.url).await?;
        let jar_override = data.jar.clone();
        data = merge_versions(data, parent_data);
        if let Some(jar) = jar_override {
            data.jar = Some(jar);
        }
    }

    Ok(data)
}

pub fn merge_versions(child: VersionData, parent: VersionData) -> VersionData {
    let mut merged = parent;

    merged.id = child.id;
    merged.r#type = child.r#type;
    merged.main_class = child.main_class;
    merged.jar = child.jar;

    if child.inherits_from.is_some() {
        merged.inherits_from = child.inherits_from;
    }
    if child.arguments.is_some() {
        merged.arguments = child.arguments;
    }
    if child.minecraft_arguments.is_some() {
        merged.minecraft_arguments = child.minecraft_arguments;
    }
    if child.asset_index.is_some() {
        merged.asset_index = child.asset_index;
    }
    if child.assets.is_some() {
        merged.assets = child.assets;
    }
    if child.downloads.is_some() {
        merged.downloads = child.downloads;
    }
    if child.logging.is_some() {
        merged.logging = child.logging;
    }
    if child.minimum_launcher_version.is_some() {
        merged.minimum_launcher_version = child.minimum_launcher_version;
    }

    let mut lib_map: HashMap<String, Library> = HashMap::new();
    for lib in merged.libraries {
        lib_map.insert(lib.name.clone(), lib);
    }
    for lib in child.libraries {
        lib_map.insert(lib.name.clone(), lib);
    }
    merged.libraries = lib_map.into_values().collect();

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arg_value_inner_as_slice_string() {
        let inner = ArgValueInner::String("test".to_string());
        assert_eq!(inner.as_slice(), vec!["test".to_string()]);
    }

    #[test]
    fn test_arg_value_inner_as_slice_array() {
        let inner = ArgValueInner::Array(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(inner.as_slice(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_merge_versions_library_dedup() {
        let child = VersionData {
            id: "1.21.4".to_string(),
            r#type: "release".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            inherits_from: None,
            arguments: None,
            minecraft_arguments: None,
            asset_index: None,
            assets: None,
            downloads: None,
            libraries: vec![Library {
                name: "com.example:lib:1.0".to_string(),
                downloads: None,
                rules: None,
                natives: None,
                extract: None,
                url: None,
                md5: None,
                classifiers: None,
            }],
            logging: None,
            jar: None,
            minimum_launcher_version: None,
        };

        let parent = VersionData {
            id: "1.21".to_string(),
            r#type: "release".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            inherits_from: None,
            arguments: None,
            minecraft_arguments: None,
            asset_index: None,
            assets: None,
            downloads: None,
            libraries: vec![Library {
                name: "com.example:lib:1.0".to_string(),
                downloads: None,
                rules: None,
                natives: None,
                extract: None,
                url: None,
                md5: None,
                classifiers: None,
            }],
            logging: None,
            jar: None,
            minimum_launcher_version: None,
        };

        let merged = merge_versions(child, parent);
        assert_eq!(merged.libraries.len(), 1);
        assert_eq!(merged.id, "1.21.4");
    }

    #[test]
    fn test_merge_versions_preserves_child_jar() {
        let child = VersionData {
            id: "forge-1.21.4".to_string(),
            r#type: "release".to_string(),
            main_class: "net.minecraft.launchwrapper.Launch".to_string(),
            inherits_from: Some("1.21.4".to_string()),
            arguments: None,
            minecraft_arguments: None,
            asset_index: None,
            assets: None,
            downloads: None,
            libraries: vec![],
            logging: None,
            jar: Some("1.21.4".to_string()),
            minimum_launcher_version: None,
        };

        let parent = VersionData {
            id: "1.21.4".to_string(),
            r#type: "release".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            inherits_from: None,
            arguments: None,
            minecraft_arguments: None,
            asset_index: None,
            assets: None,
            downloads: None,
            libraries: vec![],
            logging: None,
            jar: None,
            minimum_launcher_version: None,
        };

        let merged = merge_versions(child, parent);
        assert_eq!(merged.jar, Some("1.21.4".to_string()));
        assert_eq!(merged.main_class, "net.minecraft.launchwrapper.Launch");
    }
}
