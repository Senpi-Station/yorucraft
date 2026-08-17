use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CurseForgeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Mod not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, CurseForgeError>;

const BASE_URL: &str = "https://api.curseforge.com";
const API_KEY: &str = "$2a$10$bL4bIL5pUWqfcO7KQtnMReakwtfHbNKh6v1uTpKlzhwoueEJQnPnm";

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseMod {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub summary: String,
    #[serde(default, rename = "downloadCount")]
    pub download_count: u64,
    #[serde(default, rename = "fileFingerprint")]
    pub file_fingerprint: u64,
    #[serde(default)]
    pub categories: Vec<CurseCategory>,
    #[serde(default, rename = "latestFiles")]
    pub latest_files: Vec<CurseFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseCategory {
    pub id: u32,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseFile {
    pub id: u64,
    #[serde(default, rename = "displayName")]
    pub display_name: String,
    #[serde(default, rename = "fileName")]
    pub file_name: String,
    #[serde(default, rename = "fileLength")]
    pub file_length: u64,
    #[serde(default, rename = "downloadUrl")]
    pub download_url: String,
    #[serde(default, rename = "fileFingerprint")]
    pub file_fingerprint: u64,
    #[serde(default, rename = "gameVersions")]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<CurseFileDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseFileDependency {
    pub id: u64,
    #[serde(default, rename = "fileId")]
    pub file_id: u64,
    #[serde(default, rename = "type")]
    pub dependency_type: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseSearchResult {
    pub data: Vec<CurseMod>,
    pub pagination: CursePagination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursePagination {
    #[serde(default, rename = "totalCount")]
    pub total_count: u32,
    #[serde(default)]
    pub index: u32,
    #[serde(default, rename = "resultCount")]
    pub result_count: u32,
}

// ─── Class IDs ─────────────────────────────────────────────────────

pub const CLASS_MODS: u32 = 6;
pub const CLASS_MODPACKS: u32 = 12;
pub const CLASS_RESOURCE_PACKS: u32 = 17;

// ─── Search ────────────────────────────────────────────────────────

pub async fn search_mods(
    client: &reqwest::Client,
    query: &str,
    mc_version: Option<&str>,
    class_id: u32,
    offset: u32,
    limit: u32,
) -> Result<CurseSearchResult> {
    let mut req = client
        .get(format!("{}/v1/mods/search", BASE_URL))
        .header("x-api-key", API_KEY)
        .header("Accept", "application/json")
        .query(&[
            ("searchFilter", query),
            ("classId", &class_id.to_string()),
            ("index", &offset.to_string()),
            ("pageSize", &limit.to_string()),
        ]);

    if let Some(version) = mc_version {
        req = req.query(&[("gameVersion", version)]);
    }

    let resp = req.send().await?;

    if resp.status().as_u16() == 404 {
        return Err(CurseForgeError::NotFound(query.to_string()));
    }

    let result: CurseSearchResult = resp.json().await?;
    Ok(result)
}

// ─── Mod info ──────────────────────────────────────────────────────

pub async fn get_mod(client: &reqwest::Client, mod_id: u64) -> Result<CurseMod> {
    let resp = client
        .get(format!("{}/v1/mods/{}", BASE_URL, mod_id))
        .header("x-api-key", API_KEY)
        .header("Accept", "application/json")
        .send()
        .await?;

    if resp.status().as_u16() == 404 {
        return Err(CurseForgeError::NotFound(mod_id.to_string()));
    }

    #[derive(Deserialize)]
    struct Wrapper {
        data: CurseMod,
    }
    let wrapper: Wrapper = resp.json().await?;
    Ok(wrapper.data)
}

pub async fn get_mod_files(
    client: &reqwest::Client,
    mod_id: u64,
    mc_version: Option<&str>,
) -> Result<Vec<CurseFile>> {
    let mut req = client
        .get(format!("{}/v1/mods/{}/files", BASE_URL, mod_id))
        .header("x-api-key", API_KEY)
        .header("Accept", "application/json");

    if let Some(version) = mc_version {
        req = req.query(&[("gameVersion", version)]);
    }

    let resp = req.send().await?;

    #[derive(Deserialize)]
    struct Wrapper {
        data: Vec<CurseFile>,
    }
    let wrapper: Wrapper = resp.json().await?;
    Ok(wrapper.data)
}

// ─── Download ──────────────────────────────────────────────────────

pub async fn download_file(
    client: &reqwest::Client,
    file: &CurseFile,
    dest_dir: &Path,
    progress_fn: &dyn Fn(u64, u64),
) -> Result<PathBuf> {
    if !dest_dir.exists() {
        std::fs::create_dir_all(dest_dir)?;
    }

    let dest = dest_dir.join(&file.file_name);
    progress_fn(0, file.file_length);

    let resp = client
        .get(&file.download_url)
        .header("x-api-key", API_KEY)
        .send()
        .await?;

    let bytes = resp.bytes().await?;
    std::fs::write(&dest, &bytes)?;
    progress_fn(file.file_length, file.file_length);

    Ok(dest)
}

// ─── Modpack support ───────────────────────────────────────────────

pub async fn get_modpacks(
    client: &reqwest::Client,
    mc_version: Option<&str>,
    offset: u32,
) -> Result<CurseSearchResult> {
    search_mods(client, "", mc_version, CLASS_MODPACKS, offset, 20).await
}

pub async fn download_modpack(
    client: &reqwest::Client,
    modpack_id: u64,
    game_dir: &Path,
    progress_fn: &dyn Fn(u64, u64),
) -> Result<PathBuf> {
    let files = get_mod_files(client, modpack_id, None).await?;
    let latest = files
        .into_iter()
        .next()
        .ok_or_else(|| CurseForgeError::NotFound("no files for modpack".into()))?;

    download_file(client, &latest, game_dir, progress_fn).await
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class_ids() {
        assert_eq!(CLASS_MODS, 6);
        assert_eq!(CLASS_MODPACKS, 12);
        assert_eq!(CLASS_RESOURCE_PACKS, 17);
    }
}
