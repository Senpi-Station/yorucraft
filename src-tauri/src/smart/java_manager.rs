use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JavaManagerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Java runtime manifest is missing platform data for {platform}")]
    MissingPlatform { platform: String },
    #[error("No Java runtime available for component {component}")]
    NoRuntime { component: String },
}

pub type Result<T> = std::result::Result<T, JavaManagerError>;

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedJava {
    pub id: String,
    pub version: String,
    pub path: PathBuf,
    pub platform: String,
    pub installed: bool,
    pub size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaManifest {
    pub available: HashMap<String, HashMap<String, JavaRuntime>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaRuntime {
    pub version: String,
    pub available: HashMap<String, DownloadInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

// ─── Component mapping ─────────────────────────────────────────────

pub fn component_for_version(mc_version: &str) -> &'static str {
    let parts: Vec<&str> = mc_version.split('.').collect();
    if parts.len() < 2 {
        return "java-runtime-delta";
    }
    let minor: u32 = parts[1].parse().unwrap_or(0);
    match minor {
        8..=16 => "java-runtime-legacy",
        17 => "java-runtime-alpha",
        18..=20 => "java-runtime-gamma",
        _ => "java-runtime-delta",
    }
}

pub fn component_java_version(component: &str) -> &'static str {
    match component {
        "java-runtime-legacy" => "8",
        "java-runtime-alpha" => "16",
        "java-runtime-gamma" => "17",
        "java-runtime-delta" => "21",
        _ => "unknown",
    }
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "mac-os"
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x64"
        }
    } else {
        "linux"
    }
}

fn java_binary_name() -> &'static str {
    if cfg!(windows) { "javaw.exe" } else { "java" }
}

// ─── Manifest ──────────────────────────────────────────────────────

pub async fn get_java_manifest(client: &reqwest::Client) -> Result<JavaManifest> {
    let url = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12879/all.json";
    let resp = client.get(url).send().await?;
    let manifest: serde_json::Value = resp.json().await?;

    let mut available = HashMap::new();

    if let Some(obj) = manifest.as_object() {
        for (component, platforms) in obj {
            if let Some(platforms_obj) = platforms.as_object() {
                let mut platform_map = HashMap::new();
                for (platform, data) in platforms_obj {
                    let version = data["version"]["name"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string();
                    let mut download_map = HashMap::new();

                    if let Some(files) = data.get("files").and_then(|f| f.as_object()) {
                        for (path, info) in files {
                            if info["type"].as_str() == Some("file") {
                                if let Some(url) = info["url"].as_str() {
                                    download_map.insert(
                                        path.clone(),
                                        DownloadInfo {
                                            sha1: info["sha1"]
                                                .as_str()
                                                .unwrap_or("")
                                                .into(),
                                            size: info["size"].as_u64().unwrap_or(0),
                                            url: url.to_string(),
                                        },
                                    );
                                }
                            }
                        }
                    }

                    platform_map.insert(
                        platform.clone(),
                        JavaRuntime {
                            version,
                            available: download_map,
                        },
                    );
                }
                available.insert(component.clone(), platform_map);
            }
        }
    }

    Ok(JavaManifest { available })
}

// ─── Listing ───────────────────────────────────────────────────────

pub fn list_managed_javas(runtime_dir: &Path) -> Vec<ManagedJava> {
    let mut result = Vec::new();
    let platform = current_platform().to_string();

    for component in &[
        "java-runtime-legacy",
        "java-runtime-alpha",
        "java-runtime-gamma",
        "java-runtime-delta",
    ] {
        let component_dir = runtime_dir.join(component);
        let java_path = component_dir
            .join("bin")
            .join(java_binary_name());

        let installed = java_path.exists();
        let size_mb = if installed {
            dir_size_mb(&component_dir)
        } else {
            0
        };

        result.push(ManagedJava {
            id: component.to_string(),
            version: component_java_version(component).into(),
            path: java_path,
            platform: platform.clone(),
            installed,
            size_mb,
        });
    }

    result
}

// ─── Download ──────────────────────────────────────────────────────

pub async fn download_java(
    component: &str,
    runtime_dir: &Path,
    progress_fn: &dyn Fn(u64, u64),
) -> Result<PathBuf> {
    let client = reqwest::Client::new();
    let manifest = get_java_manifest(&client).await?;
    let platform = current_platform();

    let platform_data = manifest
        .available
        .get(component)
        .and_then(|p| p.get(platform))
        .ok_or_else(|| JavaManagerError::NoRuntime {
            component: component.to_string(),
        })?;

    let total: u64 = platform_data.available.values().map(|f| f.size).sum();
    let mut downloaded: u64 = 0;

    let component_dir = runtime_dir.join(component);

    for (path, info) in &platform_data.available {
        let target = component_dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let resp = client.get(&info.url).send().await?;
        let bytes = resp.bytes().await?;
        std::fs::write(&target, &bytes)?;

        downloaded += bytes.len() as u64;
        progress_fn(downloaded, total);
    }

    let java_path = component_dir.join("bin").join(java_binary_name());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            &java_path,
            std::fs::Permissions::from_mode(0o755),
        );
    }

    Ok(java_path)
}

// ─── Ensure for version ────────────────────────────────────────────

pub async fn ensure_java_for_version(
    mc_version: &str,
    runtime_dir: &Path,
) -> Result<PathBuf> {
    let component = component_for_version(mc_version);
    let java_path = runtime_dir
        .join(component)
        .join("bin")
        .join(java_binary_name());

    if java_path.exists() {
        return Ok(java_path);
    }

    let client = reqwest::Client::new();
    let manifest = get_java_manifest(&client).await?;
    let platform = current_platform();

    let has_runtime = manifest
        .available
        .get(component)
        .and_then(|p| p.get(platform))
        .is_some();

    if !has_runtime {
        return Err(JavaManagerError::NoRuntime {
            component: component.to_string(),
        });
    }

    download_java(component, runtime_dir, &|_, _| {}).await
}

// ─── Delete ────────────────────────────────────────────────────────

pub fn delete_java(id: &str, runtime_dir: &Path) -> Result<u64> {
    let dir = runtime_dir.join(id);
    if !dir.exists() {
        return Ok(0);
    }
    let freed = dir_size_mb(&dir);
    std::fs::remove_dir_all(&dir)?;
    Ok(freed)
}

// ─── Path resolution ───────────────────────────────────────────────

pub fn get_java_path_for_version(
    mc_version: &str,
    runtime_dir: &Path,
    system_java: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(sys) = system_java {
        if sys.exists() {
            return Some(sys.to_path_buf());
        }
    }

    let component = component_for_version(mc_version);
    let managed_path = runtime_dir
        .join(component)
        .join("bin")
        .join(java_binary_name());

    if managed_path.exists() {
        return Some(managed_path);
    }

    None
}

// ─── Helpers ───────────────────────────────────────────────────────

fn dir_size_mb(dir: &Path) -> u64 {
    fn walk(dir: &Path) -> u64 {
        let mut bytes: u64 = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    bytes += walk(&path);
                } else if let Ok(meta) = std::fs::metadata(&path) {
                    bytes += meta.len();
                }
            }
        }
        bytes
    }
    walk(dir) / (1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_for_version() {
        assert_eq!(component_for_version("1.8.9"), "java-runtime-legacy");
        assert_eq!(component_for_version("1.16.5"), "java-runtime-legacy");
        assert_eq!(component_for_version("1.17.1"), "java-runtime-alpha");
        assert_eq!(component_for_version("1.20.4"), "java-runtime-gamma");
        assert_eq!(component_for_version("1.21.4"), "java-runtime-delta");
    }

    #[test]
    fn test_component_java_version() {
        assert_eq!(component_java_version("java-runtime-legacy"), "8");
        assert_eq!(component_java_version("java-runtime-alpha"), "16");
        assert_eq!(component_java_version("java-runtime-gamma"), "17");
        assert_eq!(component_java_version("java-runtime-delta"), "21");
    }

    #[test]
    fn test_list_managed_javas_empty() {
        let dir = tempfile::tempdir().unwrap();
        let javas = list_managed_javas(dir.path());
        assert_eq!(javas.len(), 4);
        assert!(javas.iter().all(|j| !j.installed));
    }

    #[test]
    fn test_delete_java_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let freed = delete_java("java-runtime-legacy", dir.path()).unwrap();
        assert_eq!(freed, 0);
    }

    #[test]
    fn test_get_java_path_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = get_java_path_for_version("1.20.4", dir.path(), None);
        assert!(path.is_none());
    }

    #[test]
    fn test_get_java_path_system() {
        let dir = tempfile::tempdir().unwrap();
        let sys_java = dir.path().join("system-java");
        std::fs::write(&sys_java, b"").unwrap();
        let path = get_java_path_for_version("1.20.4", dir.path(), Some(&sys_java));
        assert!(path.is_some());
    }

    #[test]
    fn test_dir_size_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(dir_size_mb(dir.path()), 0);
    }

    #[test]
    fn test_dir_size_with_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.bin"), vec![0u8; 2 * 1024 * 1024]).unwrap();
        assert!(dir_size_mb(dir.path()) >= 2);
    }
}
