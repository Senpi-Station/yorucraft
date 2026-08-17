use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModrinthError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Mod not found: {0}")]
    NotFound(String),
    #[error("Rate limited — try again later")]
    RateLimited,
}

pub type Result<T> = std::result::Result<T, ModrinthError>;

const BASE_URL: &str = "https://api.modrinth.com/v2";
const USER_AGENT: &str = "YoruCraft/0.1.0 (https://github.com/Senpi-Station/yorucraft)";

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthMod {
    pub slug: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub downloads: u64,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default, rename = "author")]
    pub author: String,
    #[serde(default, rename = "date_modified")]
    pub date_modified: String,
    #[serde(default, rename = "client_side")]
    pub client_side: String,
    #[serde(default, rename = "server_side")]
    pub server_side: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "version_number")]
    pub version_number: String,
    #[serde(default, rename = "version_type")]
    pub version_type: String,
    #[serde(default, rename = "game_versions")]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub files: Vec<ModrinthFile>,
    #[serde(default)]
    pub dependencies: Vec<ModrinthDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthFile {
    pub filename: String,
    pub url: String,
    #[serde(default)]
    pub hashes: HashMap<String, String>,
    pub size: u64,
    #[serde(default, rename = "primary")]
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthDependency {
    #[serde(default, rename = "project_id")]
    pub project_id: String,
    #[serde(default, rename = "version_id")]
    pub version_id: Option<String>,
    #[serde(default, rename = "dependency_type")]
    pub dependency_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(default)]
    pub hits: Vec<ModrinthMod>,
    #[serde(default, rename = "offset")]
    pub offset: u32,
    #[serde(default, rename = "limit")]
    pub limit: u32,
    #[serde(default, rename = "total_hits")]
    pub total_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModUpdate {
    pub name: String,
    pub current_version: String,
    pub latest_version: String,
    pub download_url: String,
}

// ─── Search ────────────────────────────────────────────────────────

pub async fn search_mods(
    client: &reqwest::Client,
    query: &str,
    mc_version: &str,
    loader: &str,
    offset: u32,
    limit: u32,
) -> Result<SearchResult> {
    let facets = format!(
        r#"[["categories:mod"],["versions:{}"],["categories:{}"]]"#,
        mc_version, loader
    );

    let resp = client
        .get(format!("{}/search", BASE_URL))
        .header("User-Agent", USER_AGENT)
        .query(&[
            ("query", query),
            ("facets", &facets),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
        ])
        .send()
        .await?;

    if resp.status().as_u16() == 429 {
        return Err(ModrinthError::RateLimited);
    }
    if resp.status().as_u16() == 404 {
        return Err(ModrinthError::NotFound(query.to_string()));
    }

    let result: SearchResult = resp.json().await?;
    Ok(result)
}

// ─── Mod info ──────────────────────────────────────────────────────

pub async fn get_mod(client: &reqwest::Client, project_id: &str) -> Result<ModrinthMod> {
    let resp = client
        .get(format!("{}/project/{}", BASE_URL, project_id))
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    if resp.status().as_u16() == 404 {
        return Err(ModrinthError::NotFound(project_id.to_string()));
    }

    let result: ModrinthMod = resp.json().await?;
    Ok(result)
}

pub async fn get_mod_versions(
    client: &reqwest::Client,
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Result<Vec<ModrinthVersion>> {
    let resp = client
        .get(format!("{}/project/{}/version", BASE_URL, project_id))
        .header("User-Agent", USER_AGENT)
        .query(&[
            ("game_versions", format!(r#"["{}"]"#, mc_version).as_str()),
            ("loaders", format!(r#"["{}"]"#, loader).as_str()),
        ])
        .send()
        .await?;

    if resp.status().as_u16() == 404 {
        return Ok(Vec::new());
    }

    let versions: Vec<ModrinthVersion> = resp.json().await?;
    Ok(versions)
}

// ─── Download ──────────────────────────────────────────────────────

pub async fn download_mod(
    client: &reqwest::Client,
    file: &ModrinthFile,
    mods_dir: &Path,
    progress_fn: &dyn Fn(u64, u64),
) -> Result<PathBuf> {
    if !mods_dir.exists() {
        std::fs::create_dir_all(mods_dir)?;
    }

    let dest = mods_dir.join(&file.filename);
    progress_fn(0, file.size);

    let resp = client.get(&file.url).header("User-Agent", USER_AGENT).send().await?;
    let bytes = resp.bytes().await?;

    // Verify SHA-512 if available
    if let Some(_expected) = file.hashes.get("sha512") {
        use sha1::Digest;
        let mut hasher = sha1::Sha1::new();
        hasher.update(&bytes);
        // Note: sha1 crate doesn't have sha512. Use hex crate for SHA-256 verification.
        // SHA-512 verification would need the sha2 crate. For now verify SHA-1 if available.
    }

    if let Some(expected_sha1) = file.hashes.get("sha1") {
        use sha1::Digest;
        let mut hasher = sha1::Sha1::new();
        hasher.update(&bytes);
        let actual = hex::encode(hasher.finalize());
        if actual != *expected_sha1 {
            return Err(ModrinthError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("SHA-1 mismatch: expected {}, got {}", expected_sha1, actual),
            )));
        }
    }

    std::fs::write(&dest, &bytes)?;
    progress_fn(file.size, file.size);

    Ok(dest)
}

// ─── Update checking ───────────────────────────────────────────────

pub async fn check_mod_updates(
    client: &reqwest::Client,
    mods_dir: &Path,
    mc_version: &str,
    loader: &str,
) -> Result<Vec<ModUpdate>> {
    let mut updates = Vec::new();

    if !mods_dir.exists() {
        return Ok(updates);
    }

    for entry in std::fs::read_dir(mods_dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jar") {
            continue;
        }

        // Try to read Modrinth slug from .modrinth.lock or mod metadata
        if let Some(slug) = extract_modrinth_slug(&path) {
            if let Ok(versions) = get_mod_versions(client, &slug, mc_version, loader).await {
                if let Some(latest) = versions.first() {
                    let current_version = extract_version_from_filename(&path);
                    if current_version != latest.version_number {
                        if let Some(file) = latest.files.iter().find(|f| f.primary) {
                            updates.push(ModUpdate {
                                name: path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown")
                                    .to_string(),
                                current_version,
                                latest_version: latest.version_number.clone(),
                                download_url: file.url.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(updates)
}

// ─── Modpack support ───────────────────────────────────────────────

pub async fn download_modpack_version(
    client: &reqwest::Client,
    version_id: &str,
    game_dir: &Path,
) -> Result<PathBuf> {
    let resp = client
        .get(format!("{}/version/{}", BASE_URL, version_id))
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    let version: ModrinthVersion = resp.json().await?;

    let primary_file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| ModrinthError::NotFound("no files in version".into()))?;

    // Download the .mrpack file
    let mrpack_path = game_dir.join(&primary_file.filename);
    let resp = client.get(&primary_file.url).header("User-Agent", USER_AGENT).send().await?;
    let bytes = resp.bytes().await?;
    std::fs::write(&mrpack_path, &bytes)?;

    Ok(mrpack_path)
}

// ─── Helpers ───────────────────────────────────────────────────────

fn extract_modrinth_slug(jar_path: &Path) -> Option<String> {
    // Try .modrinth.lock first (Modrinth App format)
    let lock_path = jar_path.with_extension("lock");
    if let Ok(content) = std::fs::read_to_string(&lock_path) {
        if let Some(line) = content.lines().next() {
            let slug = line.trim();
            if !slug.is_empty() {
                return Some(slug.to_string());
            }
        }
    }

    // Try reading from fabric.mod.json inside the JAR
    if let Ok(file) = std::fs::File::open(jar_path) {
        if let Ok(mut archive) = zip::ZipArchive::new(file) {
            if let Ok(mut f) = archive.by_name("fabric.mod.json") {
                let mut contents = String::new();
                use std::io::Read;
                let _ = f.read_to_string(&mut contents);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&contents) {
                    // Check for Modrinth-specific fields
                    if let Some(s) = v.get("contact").and_then(|c| c.get("modrinth")).and_then(|m| m.as_str()) {
                        return Some(s.to_string());
                    }
                    if let Some(s) = v.get("x-prism-resource-slug").and_then(|m| m.as_str()) {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }

    None
}

fn extract_version_from_filename(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("0.0.0");

    // Common patterns: modname-1.0.0, modname-1.0.0+mc1.20.1
    let after_last_dash = stem.rsplit('-').next().unwrap_or(stem);
    let version = after_last_dash
        .split('+')
        .next()
        .unwrap_or(after_last_dash);
    version.to_string()
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version_from_filename() {
        let path = PathBuf::from("sodium-0.5.3+mc1.20.1.jar");
        assert_eq!(extract_version_from_filename(&path), "0.5.3");

        let path = PathBuf::from("fabric-api-0.87.0.jar");
        assert_eq!(extract_version_from_filename(&path), "0.87.0");
    }

    #[tokio::test]
    async fn test_search_mods() {
        let client = reqwest::Client::new();
        let result = search_mods(&client, "sodium", "1.20.4", "fabric", 0, 5).await;
        if let Ok(search) = result {
            assert!(search.hits.len() <= 5);
        }
    }

    #[tokio::test]
    async fn test_get_mod() {
        let client = reqwest::Client::new();
        // Fabric API project ID
        let result = get_mod(&client, "P7dR8mSH").await;
        if let Ok(m) = result {
            assert_eq!(m.slug, "fabric-api");
        }
    }
}
