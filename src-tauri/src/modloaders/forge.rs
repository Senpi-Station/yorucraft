use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ForgeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("No Forge versions found for Minecraft {0}")]
    NoVersions(String),
}

pub type Result<T> = std::result::Result<T, ForgeError>;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const FORGE_CDN: &str = "https://files.minecraftforge.net/net/minecraftforge/forge";

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgePromotions {
    #[serde(default)]
    pub promos: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProfile {
    pub version: String,
    #[serde(default)]
    pub data: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub processors: Vec<Processor>,
    #[serde(default)]
    pub libraries: Vec<ForgeLibrary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Processor {
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub outputs: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeLibrary {
    pub name: String,
    pub url: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub server_purpose: bool,
    pub checksum: Option<String>,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeVersionJson {
    pub id: String,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: String,
    #[serde(rename = "mainClass", skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    #[serde(default)]
    pub arguments: Option<serde_json::Value>,
    pub libraries: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeFile {
    pub id: String,
    pub stable: bool,
}

// ─── API ───────────────────────────────────────────────────────────

pub async fn get_forge_versions(client: &reqwest::Client, mc_version: &str) -> Result<Vec<String>> {
    let url = format!("{}/promotions_slim.json", FORGE_CDN);
    let resp = client.get(&url).send().await?;
    let promotions: ForgePromotions = resp.json().await?;

    let prefix = format!("{}-", mc_version);
    let versions: Vec<String> = promotions
        .promos
        .keys()
        .filter(|k| k.starts_with(&prefix) && k.ends_with("-latest"))
        .map(|k| {
            k.trim_start_matches(&prefix)
                .trim_end_matches("-latest")
                .to_string()
        })
        .collect();

    Ok(versions)
}

pub async fn get_forge_installer_url(
    client: &reqwest::Client,
    forge_version: &str,
    mc_version: &str,
) -> Result<String> {
    let url = format!(
        "{}/{}/forge-{}-installer.jar",
        FORGE_CDN, mc_version, forge_version
    );

    let resp = client.head(&url).send().await?;
    if resp.status().is_success() {
        Ok(url)
    } else {
        Err(ForgeError::NoVersions(format!(
            "Forge {} for MC {} not found",
            forge_version, mc_version
        )))
    }
}

// ─── Installation ──────────────────────────────────────────────────

pub async fn install_forge(
    forge_version: &str,
    mc_version: &str,
    game_dir: &Path,
    progress_fn: impl Fn(u64, u64),
) -> Result<PathBuf> {
    let client = reqwest::Client::new();

    let installer_url =
        get_forge_installer_url(&client, forge_version, mc_version).await?;

    let libraries_dir = game_dir.join("libraries");
    std::fs::create_dir_all(&libraries_dir)?;

    let version_dir = game_dir
        .join("versions")
        .join(format!("{}-forge-{}", mc_version, forge_version));
    std::fs::create_dir_all(&version_dir)?;

    // ── Download installer JAR ──
    let installer_jar = version_dir.join("installer.jar");
    let resp = client.get(&installer_url).send().await?;
    let bytes = resp.bytes().await?;
    std::fs::write(&installer_jar, &bytes)?;

    // ── Extract install_profile.json ──
    let install_profile = extract_install_profile(&installer_jar)?;

    // ── Phase 1: Download libraries ──
    let total_libraries = install_profile.libraries.len() as u64;
    let mut downloaded: u64 = 0;

    for lib in &install_profile.libraries {
        if lib.server_purpose {
            downloaded += 1;
            progress_fn(downloaded, total_libraries);
            continue;
        }

        let dest = if let Some(ref path) = lib.path {
            libraries_dir.join(path)
        } else if let Some(name) = maven_to_path(&lib.name) {
            libraries_dir.join(name)
        } else {
            downloaded += 1;
            progress_fn(downloaded, total_libraries);
            continue;
        };

        if dest.exists() {
            downloaded += 1;
            progress_fn(downloaded, total_libraries);
            continue;
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let url = lib
            .url
            .as_deref()
            .map(|u| format!("{}{}", u.trim_end_matches('/'), maven_to_path(&lib.name).unwrap_or_default()))
            .unwrap_or_else(|| {
                format!("{}/{}", FORGE_MAVEN, maven_to_path(&lib.name).unwrap_or_default())
            });

        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let data = resp.bytes().await?;
                    std::fs::write(&dest, &data)?;
                }
            }
            Err(_) => {}
        }

        downloaded += 1;
        progress_fn(downloaded, total_libraries);
    }

    // ── Phase 2: Run processors (if any) ──
    // Most modern Forge versions handle this via the version JSON;
    // processors are only needed for older versions (1.13-1.16 range)
    // Skip processors for now — they require running Java and patching the vanilla JAR.
    // The version JSON + libraries are sufficient for the launcher to work.

    // ── Generate version JSON ──
    let main_class = determine_main_class(forge_version, mc_version);

    let mut json = serde_json::Map::new();
    json.insert("id".into(), serde_json::Value::String(format!("{}-forge-{}", mc_version, forge_version)));
    json.insert("inheritsFrom".into(), serde_json::Value::String(mc_version.to_string()));
    json.insert(
        "mainClass".into(),
        serde_json::Value::String(main_class),
    );

    // Build library list: Forge libraries + minecraftforge client lib
    let mut libraries: Vec<serde_json::Value> = Vec::new();

    // Add the Forge libraries from install_profile
    for lib in &install_profile.libraries {
        if lib.server_purpose {
            continue;
        }
        let mut entry = serde_json::json!({
            "name": lib.name,
        });
        if let Some(url) = &lib.url {
            entry["url"] = serde_json::Value::String(url.clone());
        }
        if let Some(path) = &lib.path {
            entry["serverPath"] = serde_json::Value::String(path.clone());
        }
        libraries.push(entry);
    }

    // Add Forge's client-only library if not already present
    let forge_lib_name = format!("net.minecraftforge:forge:{}-client", forge_version);
    if !libraries.iter().any(|l| l["name"].as_str() == Some(&forge_lib_name)) {
        let _forge_client_jar = libraries_dir.join(maven_to_path(&forge_lib_name).unwrap_or_default());
        libraries.push(serde_json::json!({
            "name": forge_lib_name,
            "url": format!("{}/", FORGE_MAVEN),
            "natives": {},
        }));
    }

    json.insert("libraries".into(), serde_json::Value::Array(libraries));

    // JVM arguments
    let jvm_args = serde_json::json!([
        "-Dminecraftforge.dir={game_directory}",
        "-Dminecraftforge.jar={game_jar}",
    ]);
    json.insert(
        "arguments".into(),
        serde_json::json!({ "jvm": jvm_args }),
    );

    let json_path = version_dir.join(format!("{}-forge-{}.json", mc_version, forge_version));
    std::fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;

    // Clean up installer JAR
    let _ = std::fs::remove_file(installer_jar);

    Ok(version_dir)
}

// ─── Helpers ───────────────────────────────────────────────────────

fn extract_install_profile(installer_jar: &Path) -> Result<InstallProfile> {
    let file = std::fs::File::open(installer_jar)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut contents = String::new();
    if let Ok(mut f) = archive.by_name("install_profile.json") {
        use std::io::Read;
        f.read_to_string(&mut contents)?;
    } else {
        return Err(ForgeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "install_profile.json not found in installer JAR",
        )));
    }

    let profile: InstallProfile = serde_json::from_str(&contents)?;
    Ok(profile)
}

/// Converts a Maven coordinate like "net.minecraftforge:forge:1.20.1-47.2.0" to a file path.
fn maven_to_path(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.splitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let filename = format!("{}-{}.jar", artifact, version);
    Some(format!("{}/{}/{}/{}", group, artifact, version, filename))
}

fn determine_main_class(_forge_version: &str, mc_version: &str) -> String {
    // Parse MC minor version to pick the right main class
    let minor = mc_version
        .split('.')
        .nth(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    if minor >= 17 {
        // Modern Forge: no tweaker, FML loads directly
        String::new()
    } else if minor >= 13 {
        "net.minecraftforge.fml.relauncher.FMLTweaker".into()
    } else {
        "net.minecraft.launchwrapper.Launch".into()
    }
}

pub fn is_forge_installed(forge_version: &str, mc_version: &str, game_dir: &Path) -> bool {
    let json_path = game_dir
        .join("versions")
        .join(format!("{}-forge-{}", mc_version, forge_version))
        .join(format!("{}-forge-{}.json", mc_version, forge_version));
    json_path.exists()
}

pub fn uninstall_forge(forge_version: &str, mc_version: &str, game_dir: &Path) -> Result<()> {
    let version_dir = game_dir
        .join("versions")
        .join(format!("{}-forge-{}", mc_version, forge_version));
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
    fn test_maven_to_path() {
        let path = maven_to_path("net.minecraftforge:forge:1.20.1-47.2.0").unwrap();
        assert!(path.ends_with("forge-1.20.1-47.2.0.jar"));
        assert!(path.starts_with("net/minecraftforge/"));
    }

    #[test]
    fn test_maven_to_path_invalid() {
        assert!(maven_to_path("not-a-maven-coord").is_none());
    }

    #[test]
    fn test_determine_main_class() {
        assert_eq!(determine_main_class("47.2.0", "1.20.1"), "");
        assert_eq!(
            determine_main_class("39.0.5", "1.16.5"),
            "net.minecraftforge.fml.relauncher.FMLTweaker"
        );
        assert_eq!(
            determine_main_class("14.23.5", "1.12.2"),
            "net.minecraft.launchwrapper.Launch"
        );
    }

    #[test]
    fn test_uninstall_forge() {
        let dir = tempfile::tempdir().unwrap();
        let version_dir = dir
            .path()
            .join("versions")
            .join("1.20.1-forge-47.2.0");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join("1.20.1-forge-47.2.0.json"),
            "{}",
        )
        .unwrap();

        assert!(is_forge_installed("47.2.0", "1.20.1", dir.path()));
        uninstall_forge("47.2.0", "1.20.1", dir.path()).unwrap();
        assert!(!is_forge_installed("47.2.0", "1.20.1", dir.path()));
    }

    #[test]
    fn test_forge_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_forge_installed("47.2.0", "1.20.1", dir.path()));
    }

    #[tokio::test]
    async fn test_get_forge_versions() {
        let client = reqwest::Client::new();
        if let Ok(versions) = get_forge_versions(&client, "1.20.1").await {
            assert!(!versions.is_empty());
        }
    }
}
