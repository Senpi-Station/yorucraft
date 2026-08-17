use std::collections::HashMap;
use std::path::{Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::installer::manifest::*;

#[derive(Error, Debug)]
pub enum VersionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("Hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("Version not found: {0}")]
    VersionNotFound(String),
    #[error("No client download for version: {0}")]
    NoClientDownload(String),
    #[error("No asset index for version: {0}")]
    NoAssetIndex(String),
}

pub type Result<T> = std::result::Result<T, VersionError>;

// ─── Asset index structure ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexFile {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

// ─── Version Manager ───────────────────────────────────────────────

pub struct VersionManager {
    client: Client,
    game_dir: PathBuf,
}

impl VersionManager {
    pub fn new(client: Client, game_dir: PathBuf) -> Self {
        Self { client, game_dir }
    }

    fn versions_dir(&self) -> PathBuf {
        self.game_dir.join("versions")
    }

    fn libraries_dir(&self) -> PathBuf {
        self.game_dir.join("libraries")
    }

    fn assets_dir(&self) -> PathBuf {
        self.game_dir.join("assets")
    }

    // ─── Install pipeline ──────────────────────────────────────────

    pub async fn install_version(
        &self,
        version_id: &str,
        progress_fn: &dyn Fn(String, u64, u64),
    ) -> Result<PathBuf> {
        progress_fn(format!("Resolving {}", version_id), 0, 8);

        let data = resolve_version(&self.client, version_id).await?;
        let version_dir = self.versions_dir().join(&data.id);
        std::fs::create_dir_all(&version_dir)?;

        progress_fn("Downloading client JAR".into(), 1, 8);
        self.download_client_jar(&data, &version_dir).await?;

        progress_fn("Downloading asset index".into(), 2, 8);
        let asset_index_path = self.download_asset_index(&data).await?;

        progress_fn("Downloading assets".into(), 3, 8);
        self.download_assets(&data, &asset_index_path, progress_fn).await?;

        progress_fn("Downloading libraries".into(), 6, 8);
        let _libs = self.download_libraries(&data, progress_fn).await?;

        progress_fn("Extracting natives".into(), 7, 8);
        self.extract_natives(&data, &version_dir)?;

        // Write version JSON
        let json_path = version_dir.join(format!("{}.json", data.id));
        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(json_path, json)?;

        progress_fn("Done".into(), 8, 8);
        Ok(version_dir)
    }

    // ─── Client JAR ────────────────────────────────────────────────

    pub async fn download_client_jar(
        &self,
        version: &VersionData,
        version_dir: &Path,
    ) -> Result<PathBuf> {
        let dl = version
            .downloads
            .as_ref()
            .and_then(|d| d.client.as_ref())
            .ok_or_else(|| VersionError::NoClientDownload(version.id.clone()))?;

        let jar_name = format!("{}.jar", version.id);
        let jar_path = version_dir.join(&jar_name);

        if jar_path.exists() {
            if let Ok(content) = std::fs::read(&jar_path) {
                let hex = sha1_hex(&content);
                if hex == dl.sha1 {
                    return Ok(jar_path);
                }
            }
        }

        self.download_file_with_hash(&dl.url, &dl.sha1, &jar_path, dl.size)
            .await?;
        Ok(jar_path)
    }

    // ─── Asset index ───────────────────────────────────────────────

    pub async fn download_asset_index(&self, version: &VersionData) -> Result<PathBuf> {
        let ai = version
            .asset_index
            .as_ref()
            .ok_or_else(|| VersionError::NoAssetIndex(version.id.clone()))?;

        let indexes_dir = self.assets_dir().join("indexes");
        std::fs::create_dir_all(&indexes_dir)?;

        let index_path = indexes_dir.join(format!("{}.json", ai.id));

        if index_path.exists() {
            if let Ok(content) = std::fs::read(&index_path) {
                let hex = sha1_hex(&content);
                if hex == ai.sha1 {
                    return Ok(index_path);
                }
            }
        }

        self.download_file_with_hash(&ai.url, &ai.sha1, &index_path, ai.size)
            .await?;
        Ok(index_path)
    }

    // ─── Assets ────────────────────────────────────────────────────

    pub async fn download_assets(
        &self,
        _version: &VersionData,
        asset_index_path: &Path,
        progress_fn: &dyn Fn(String, u64, u64),
    ) -> Result<()> {
        let index_content = std::fs::read_to_string(asset_index_path)?;
        let index: AssetIndexFile = serde_json::from_str(&index_content)?;

        let objects_dir = self.assets_dir().join("objects");
        std::fs::create_dir_all(&objects_dir)?;

        let entries: Vec<_> = index.objects.into_iter().collect();
        let total = entries.len() as u64;
        let mut downloaded = 0u64;

        use std::sync::Arc;
        let semaphore = Arc::new(tokio::sync::Semaphore::new(16));
        let mut handles = Vec::new();

        for (name, obj) in entries {
            let hash = obj.hash.clone();
            let size = obj.size;
            let objects_dir = objects_dir.clone();
            let client = self.client.clone();
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let asset_name = name.clone();

            let handle = tokio::spawn(async move {
                let asset_path = objects_dir.join(&hash[0..2]).join(&hash);

                if asset_path.exists() {
                    if let Ok(content) = std::fs::read(&asset_path) {
                        if sha1_hex(&content) == hash {
                            drop(permit);
                            return Ok::<(), VersionError>(());
                        }
                    }
                }

                if let Some(parent) = asset_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let url = format!(
                    "https://resources.download.minecraft.net/{}/{hash}",
                    &hash[0..2]
                );
                let resp = client.get(&url).send().await?;
                let bytes = resp.bytes().await?;

                let actual = sha1_hex(&bytes);
                if actual != hash {
                    return Err(VersionError::HashMismatch {
                        path: asset_name,
                        expected: hash,
                        actual,
                    });
                }

                std::fs::write(&asset_path, &bytes)?;
                drop(permit);
                Ok(())
            });

            handles.push((name, handle, size));
        }

        for (name, handle, _size) in handles {
            downloaded += 1;
            progress_fn(
                format!("Asset: {}", name),
                downloaded,
                total,
            );
            handle.await.map_err(|e| {
                VersionError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })??;
        }

        Ok(())
    }

    // ─── Libraries ─────────────────────────────────────────────────

    pub async fn download_libraries(
        &self,
        version: &VersionData,
        progress_fn: &dyn Fn(String, u64, u64),
    ) -> Result<Vec<PathBuf>> {
        let libs_dir = self.libraries_dir();
        std::fs::create_dir_all(&libs_dir)?;

        let platform = current_platform();
        let mut paths = Vec::new();
        let total = version.libraries.len() as u64;

        for (i, lib) in version.libraries.iter().enumerate() {
            if !evaluate_rules(&lib.rules) {
                continue;
            }

            // Main artifact
            if let Some(ref dl) = lib.downloads {
                if let (Some(ref url), Some(ref path)) = (&dl.url, &dl.path) {
                    let target = libs_dir.join(path);
                    if !target.exists() {
                        let sha = dl.sha1.as_deref().unwrap_or("");
                        let size = dl.size.unwrap_or(0);
                        progress_fn(format!("Lib: {}", lib.name), i as u64, total);
                        self.download_file_with_hash(url, sha, &target, size)
                            .await?;
                    }
                    paths.push(target);
                }
            }

            // Native classifier
            if let Some(ref natives) = lib.natives {
                if let Some(classifier_key) = natives.get(&platform) {
                    let key = classifier_key.replace("${arch}", "64");
                    if let Some(ref classifiers) = lib.classifiers {
                        if let Some(ref native_dl) = classifiers.get(&key) {
                            if let (Some(ref url), Some(ref path)) =
                                (&native_dl.url, &native_dl.path)
                            {
                                let target = libs_dir.join(path);
                                if !target.exists() {
                                    progress_fn(
                                        format!("Native: {}", lib.name),
                                        i as u64,
                                        total,
                                    );
                                    self.download_file(url, &target).await?;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Add client JAR to classpath
        let jar_name = format!("{}.jar", version.id);
        let jar_path = self.versions_dir().join(&version.id).join(&jar_name);
        if jar_path.exists() {
            paths.push(jar_path);
        }

        Ok(paths)
    }

    // ─── Natives extraction ────────────────────────────────────────

    pub fn extract_natives(
        &self,
        version: &VersionData,
        version_dir: &Path,
    ) -> Result<PathBuf> {
        let natives_dir = version_dir.join("natives");
        std::fs::create_dir_all(&natives_dir)?;

        let platform = current_platform();
        let libs_dir = self.libraries_dir();

        for lib in &version.libraries {
            if !evaluate_rules(&lib.rules) {
                continue;
            }

            let Some(ref natives) = lib.natives else {
                continue;
            };

            let Some(classifier_key) = natives.get(&platform) else {
                continue;
            };

            let key = classifier_key.replace("${arch}", "64");

            let Some(ref classifiers) = lib.classifiers else {
                continue;
            };

            let Some(ref native_dl) = classifiers.get(&key) else {
                continue;
            };

            let Some(ref path) = native_dl.path else {
                continue;
            };

            let jar_path = libs_dir.join(path);
            if !jar_path.exists() {
                continue;
            }

            let excludes: Vec<String> = lib
                .extract
                .as_ref()
                .and_then(|e| e.exclude.clone())
                .unwrap_or_default();

            extract_jar_to(&jar_path, &natives_dir, &excludes)?;
        }

        Ok(natives_dir)
    }

    // ─── Classpath ─────────────────────────────────────────────────

    pub fn build_classpath(
        &self,
        version: &VersionData,
        library_paths: &[PathBuf],
    ) -> String {
        let sep = if cfg!(target_os = "windows") {
            ";"
        } else {
            ":"
        };

        let mut entries: Vec<String> = library_paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        // Add client JAR
        let jar_name = format!("{}.jar", version.jar.as_ref().unwrap_or(&version.id));
        let jar_path = self.versions_dir()
            .join(version.jar.as_ref().unwrap_or(&version.id))
            .join(&jar_name);
        if jar_path.exists() {
            entries.push(jar_path.to_string_lossy().into_owned());
        }

        entries.join(sep)
    }

    // ─── Version management ────────────────────────────────────────

    pub fn list_installed_versions(&self) -> Vec<String> {
        let versions_dir = self.versions_dir();
        if !versions_dir.exists() {
            return Vec::new();
        }

        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&versions_dir).into_iter().flatten().flatten() {
            if entry.path().is_dir() {
                let json = entry.path().join(format!("{}.json", entry.file_name().to_string_lossy()));
                let jar = entry.path().join(format!("{}.jar", entry.file_name().to_string_lossy()));
                if json.exists() || jar.exists() {
                    ids.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        ids
    }

    pub fn is_version_installed(&self, version_id: &str) -> bool {
        let vdir = self.versions_dir().join(version_id);
        let json = vdir.join(format!("{}.json", version_id));
        let jar = vdir.join(format!("{}.jar", version_id));
        json.exists() || jar.exists()
    }

    pub fn delete_version(&self, version_id: &str) -> Result<()> {
        let vdir = self.versions_dir().join(version_id);
        if vdir.exists() {
            std::fs::remove_dir_all(vdir)?;
        }
        Ok(())
    }

    // ─── Download helpers ──────────────────────────────────────────

    async fn download_file_with_hash(
        &self,
        url: &str,
        expected_sha1: &str,
        dest: &Path,
        _size: u64,
    ) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let resp = self.client.get(url).send().await?;
        let bytes = resp.bytes().await?;

        if !expected_sha1.is_empty() {
            let actual = sha1_hex(&bytes);
            if actual != expected_sha1 {
                return Err(VersionError::HashMismatch {
                    path: dest.display().to_string(),
                    expected: expected_sha1.to_string(),
                    actual,
                });
            }
        }

        std::fs::write(dest, &bytes)?;
        Ok(())
    }

    async fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let resp = self.client.get(url).send().await?;
        let bytes = resp.bytes().await?;
        std::fs::write(dest, &bytes)?;
        Ok(())
    }
}

// ─── Utility functions ─────────────────────────────────────────────

fn sha1_hex(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

fn current_platform() -> String {
    if cfg!(target_os = "linux") {
        "linux".into()
    } else if cfg!(target_os = "macos") {
        "osx".into()
    } else if cfg!(target_os = "windows") {
        "windows".into()
    } else {
        "linux".into()
    }
}

fn evaluate_rules(rules: &Option<Vec<Rule>>) -> bool {
    let Some(rules) = rules else {
        return true;
    };

    if rules.is_empty() {
        return true;
    }

    let os_name = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };

    let mut allowed = false;
    for rule in rules {
        let os_match = rule
            .os
            .as_ref()
            .map(|os| {
                os.name.as_deref() == Some(os_name)
            })
            .unwrap_or(true);

        if rule.action == "allow" && os_match {
            allowed = true;
        } else if rule.action == "disallow" && os_match {
            allowed = false;
        }
    }

    allowed
}

fn extract_jar_to(jar_path: &Path, dest_dir: &Path, excludes: &[String]) -> Result<()> {
    let file = std::fs::File::open(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        if let Ok(mut entry) = archive.by_index(i) {
            let name = entry.name().to_string();

            if name.starts_with("META-INF/") {
                continue;
            }

            if excludes.iter().any(|e| name.starts_with(e.as_str())) {
                continue;
            }

            let out_path = dest_dir.join(&name);

            if name.ends_with('/') {
                std::fs::create_dir_all(&out_path)?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut content = Vec::new();
                use std::io::Read;
                entry.read_to_end(&mut content)?;
                std::fs::write(&out_path, &content)?;
            }
        }
    }

    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_hex() {
        let data = b"hello world";
        let hex = sha1_hex(data);
        assert_eq!(hex.len(), 40);
        assert_eq!(hex, "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed");
    }

    #[test]
    fn test_current_platform() {
        let p = current_platform();
        assert!(p == "linux" || p == "osx" || p == "windows");
    }

    #[test]
    fn test_evaluate_rules_none() {
        assert!(evaluate_rules(&None));
    }

    #[test]
    fn test_evaluate_rules_empty() {
        assert!(evaluate_rules(&Some(vec![])));
    }

    #[test]
    fn test_evaluate_rules_allow_linux() {
        let rules = vec![Rule {
            action: "allow".into(),
            os: Some(OsRule {
                name: Some("linux".into()),
                version: None,
                arch: None,
            }),
            features: None,
        }];
        assert!(evaluate_rules(&Some(rules)));
    }

    #[test]
    fn test_evaluate_rules_disallow_windows() {
        let rules = vec![Rule {
            action: "disallow".into(),
            os: Some(OsRule {
                name: Some("windows".into()),
                version: None,
                arch: None,
            }),
            features: None,
        }];
        #[cfg(target_os = "windows")]
        assert!(!evaluate_rules(&Some(rules)));
        #[cfg(not(target_os = "windows"))]
        assert!(evaluate_rules(&Some(rules)));
    }
}
