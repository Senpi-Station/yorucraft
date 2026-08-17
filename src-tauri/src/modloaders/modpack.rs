use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModpackError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Unknown modpack format")]
    UnknownFormat,
    #[error("Missing required field: {0}")]
    MissingField(String),
}

pub type Result<T> = std::result::Result<T, ModpackError>;

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackManifest {
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: String,
    #[serde(default)]
    pub mods: Vec<ModpackMod>,
    #[serde(default)]
    pub overrides: Vec<OverrideFile>,
    pub settings: Option<ModpackSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackMod {
    pub source: ModSource,
    pub filename: String,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default = "default_both")]
    pub side: String,
}

fn default_true() -> bool {
    true
}
fn default_both() -> String {
    "both".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ModSource {
    #[serde(rename = "curseforge")]
    CurseForge {
        project_id: u64,
        file_id: u64,
    },
    #[serde(rename = "modrinth")]
    Modrinth {
        project_id: String,
        version_id: String,
    },
    #[serde(rename = "url")]
    Url { url: String },
    #[serde(rename = "local")]
    Local { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideFile {
    pub source_path: String,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackSettings {
    pub recommended_memory: Option<String>,
    pub java_args: Option<Vec<String>>,
}

// ─── Format detection ──────────────────────────────────────────────

pub enum ModpackFormat {
    Modrinth,
    CurseForge,
    MultiMC,
    Generic,
}

fn detect_format(archive: &mut zip::ZipArchive<std::fs::File>) -> ModpackFormat {
    let names: Vec<String> = archive
        .file_names()
        .map(String::from)
        .collect();

    if names.iter().any(|n| n.contains("modpack.index.json")) {
        ModpackFormat::Modrinth
    } else if names.iter().any(|n| n == "manifest.json") {
        ModpackFormat::CurseForge
    } else if names.iter().any(|n| n == "modpack.json" || n == "mmc-pack.json") {
        ModpackFormat::MultiMC
    } else {
        ModpackFormat::Generic
    }
}

// ─── Import entry point ────────────────────────────────────────────

pub fn import_modpack(
    file_path: &Path,
    game_dir: &Path,
    progress_fn: &dyn Fn(String, u64, u64),
) -> Result<ModpackManifest> {
    let file = std::fs::File::open(file_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let format = detect_format(&mut archive);

    match format {
        ModpackFormat::Modrinth => import_modrinth_modpack(&mut archive, game_dir, progress_fn),
        ModpackFormat::CurseForge => import_curseforge_modpack(&mut archive, game_dir, progress_fn),
        ModpackFormat::MultiMC => import_multimc_modpack(&mut archive, game_dir, progress_fn),
        ModpackFormat::Generic => import_generic_modpack(&mut archive, game_dir, progress_fn),
    }
}

// ─── Modrinth .mrpack ──────────────────────────────────────────────

fn import_modrinth_modpack(
    archive: &mut zip::ZipArchive<std::fs::File>,
    game_dir: &Path,
    progress_fn: &dyn Fn(String, u64, u64),
) -> Result<ModpackManifest> {
    progress_fn("Reading modpack manifest".into(), 0, 100);

    let mut index_content = String::new();
    {
        let mut f = archive.by_name("modpack.index.json")?;
        use std::io::Read;
        f.read_to_string(&mut index_content)?;
    }

    let index: serde_json::Value = serde_json::from_str(&index_content)?;

    let name = index["name"].as_str().unwrap_or("Unknown Modpack").to_string();
    let mc_version = index
        .get("dependencies")
        .and_then(|d| d.get("minecraft"))
        .and_then(|v| v.as_str())
        .unwrap_or("1.20.1")
        .to_string();

    let (loader, loader_version) = detect_loader_from_mrpack(index.get("dependencies"));

    let mut mods = Vec::new();
    let mut overrides = Vec::new();

    if let Some(files) = index.get("files").and_then(|f| f.as_array()) {
        let total = files.len() as u64;
        for (i, file_entry) in files.iter().enumerate() {
            let path = file_entry["path"].as_str().unwrap_or("");
            let downloads = file_entry
                .get("downloads")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                .and_then(|u| u.as_str())
                .unwrap_or("");

            if !downloads.is_empty() && path.starts_with("mods/") {
                let filename = path.rsplit('/').next().unwrap_or(path).to_string();
                mods.push(ModpackMod {
                    source: ModSource::Url {
                        url: downloads.to_string(),
                    },
                    filename,
                    required: true,
                    side: "both".into(),
                });
            }

            progress_fn(
                format!("Processing {}", path),
                (i + 1) as u64,
                total,
            );
        }
    }

    // Extract overrides
    let overrides_path = index
        .get("overrides")
        .and_then(|o| o.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or("overrides");

    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let entry_name = entry.name().to_string();
            if entry_name.starts_with(overrides_path) && !entry.is_dir() {
                let relative = entry_name
                    .strip_prefix(&format!("{}/", overrides_path))
                    .unwrap_or(&entry_name)
                    .to_string();

                let target = game_dir.join(&relative);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut content = Vec::new();
                use std::io::Read;
                let mut entry_ref = entry;
                entry_ref.read_to_end(&mut content)?;
                std::fs::write(&target, &content)?;

                overrides.push(OverrideFile {
                    source_path: entry_name,
                    target_path: relative,
                });
            }
        }
    }

    progress_fn("Modpack imported".into(), 100, 100);

    Ok(ModpackManifest {
        name,
        mc_version,
        loader,
        loader_version,
        mods,
        overrides,
        settings: None,
    })
}

// ─── CurseForge export ─────────────────────────────────────────────

fn import_curseforge_modpack(
    archive: &mut zip::ZipArchive<std::fs::File>,
    game_dir: &Path,
    progress_fn: &dyn Fn(String, u64, u64),
) -> Result<ModpackManifest> {
    progress_fn("Reading CurseForge manifest".into(), 0, 100);

    let mut manifest_content = String::new();
    {
        let mut f = archive.by_name("manifest.json")?;
        use std::io::Read;
        f.read_to_string(&mut manifest_content)?;
    }

    let manifest: serde_json::Value = serde_json::from_str(&manifest_content)?;

    let name = manifest["name"].as_str().unwrap_or("Unknown Modpack").to_string();
    let mc_version = manifest
        .get("minecraft")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("1.20.1")
        .to_string();

    let (loader, loader_version) = detect_loader_from_cf(
        manifest
            .get("minecraft")
            .and_then(|m| m.get("modLoaders")),
    );

    let mut mods = Vec::new();
    let mut overrides = Vec::new();
    let overrides_dir = manifest
        .get("overrides")
        .and_then(|o| o.as_str())
        .unwrap_or("overrides");

    if let Some(files) = manifest.get("files").and_then(|f| f.as_array()) {
        let total = files.len() as u64;
        for (i, file_entry) in files.iter().enumerate() {
            let project_id = file_entry["projectID"].as_u64().unwrap_or(0);
            let file_id = file_entry["fileID"].as_u64().unwrap_or(0);

            if project_id > 0 && file_id > 0 {
                mods.push(ModpackMod {
                    source: ModSource::CurseForge {
                        project_id,
                        file_id,
                    },
                    filename: format!("cf-{}-{}.jar", project_id, file_id),
                    required: file_entry["required"].as_bool().unwrap_or(true),
                    side: "both".into(),
                });
            }

            progress_fn(
                format!("Processing mod {}/{}", i + 1, total),
                (i + 1) as u64,
                total,
            );
        }
    }

    // Extract overrides
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let entry_name = entry.name().to_string();
            if entry_name.starts_with(overrides_dir) && !entry.is_dir() {
                let relative = entry_name
                    .strip_prefix(&format!("{}/", overrides_dir))
                    .unwrap_or(&entry_name)
                    .to_string();

                let target = game_dir.join(&relative);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut content = Vec::new();
                use std::io::Read;
                let mut entry_ref = entry;
                entry_ref.read_to_end(&mut content)?;
                std::fs::write(&target, &content)?;

                overrides.push(OverrideFile {
                    source_path: entry_name,
                    target_path: relative,
                });
            }
        }
    }

    progress_fn("CurseForge modpack imported".into(), 100, 100);

    Ok(ModpackManifest {
        name,
        mc_version,
        loader,
        loader_version,
        mods,
        overrides,
        settings: None,
    })
}

// ─── MultiMC format ────────────────────────────────────────────────

fn import_multimc_modpack(
    archive: &mut zip::ZipArchive<std::fs::File>,
    game_dir: &Path,
    progress_fn: &dyn Fn(String, u64, u64),
) -> Result<ModpackManifest> {
    progress_fn("Reading MultiMC manifest".into(), 0, 100);

    let mut mmc_content = String::new();
    let manifest_key = if archive.by_name("mmc-pack.json").is_ok() {
        "mmc-pack.json"
    } else {
        "modpack.json"
    };
    {
        let mut f = archive.by_name(manifest_key)?;
        use std::io::Read;
        f.read_to_string(&mut mmc_content)?;
    }

    let manifest: serde_json::Value = serde_json::from_str(&mmc_content)?;

    let name = manifest["name"].as_str().unwrap_or("Unknown Modpack").to_string();
    let mc_version = manifest["mcVersion"].as_str().unwrap_or("1.20.1").to_string();

    // Extract mods and config
    let mut mods = Vec::new();
    let mut overrides = Vec::new();

    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let entry_name = entry.name().to_string();

            if entry_name.starts_with("mods/") && entry_name.ends_with(".jar") && !entry.is_dir() {
                let filename = entry_name.rsplit('/').next().unwrap_or(&entry_name).to_string();
                let target = game_dir.join("mods").join(&filename);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut content = Vec::new();
                use std::io::Read;
                let mut entry_ref = entry;
                entry_ref.read_to_end(&mut content)?;
                std::fs::write(&target, &content)?;

                mods.push(ModpackMod {
                    source: ModSource::Local {
                        path: target.to_string_lossy().into(),
                    },
                    filename,
                    required: true,
                    side: "client".into(),
                });
            } else if entry_name.starts_with("config/") && !entry.is_dir() {
                let relative = entry_name.strip_prefix("config/").unwrap_or(&entry_name).to_string();
                let target = game_dir.join("config").join(&relative);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut content = Vec::new();
                use std::io::Read;
                let mut entry_ref = entry;
                entry_ref.read_to_end(&mut content)?;
                std::fs::write(&target, &content)?;

                overrides.push(OverrideFile {
                    source_path: entry_name,
                    target_path: format!("config/{}", relative),
                });
            }
        }
    }

    progress_fn("MultiMC modpack imported".into(), 100, 100);

    Ok(ModpackManifest {
        name,
        mc_version,
        loader: "vanilla".into(),
        loader_version: String::new(),
        mods,
        overrides,
        settings: None,
    })
}

// ─── Generic format ────────────────────────────────────────────────

fn import_generic_modpack(
    archive: &mut zip::ZipArchive<std::fs::File>,
    game_dir: &Path,
    progress_fn: &dyn Fn(String, u64, u64),
) -> Result<ModpackManifest> {
    progress_fn("Importing generic modpack".into(), 0, 100);

    let mut mods = Vec::new();
    let mut overrides = Vec::new();

    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let entry_name = entry.name().to_string();

            if entry_name.starts_with("mods/") && entry_name.ends_with(".jar") && !entry.is_dir() {
                let filename = entry_name.rsplit('/').next().unwrap_or(&entry_name).to_string();
                let target = game_dir.join("mods").join(&filename);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut content = Vec::new();
                use std::io::Read;
                let mut entry_ref = entry;
                entry_ref.read_to_end(&mut content)?;
                std::fs::write(&target, &content)?;

                mods.push(ModpackMod {
                    source: ModSource::Local {
                        path: target.to_string_lossy().into(),
                    },
                    filename,
                    required: true,
                    side: "client".into(),
                });
            } else if entry_name.starts_with("config/") && !entry.is_dir() {
                let relative = entry_name.strip_prefix("config/").unwrap_or(&entry_name).to_string();
                let target = game_dir.join("config").join(&relative);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut content = Vec::new();
                use std::io::Read;
                let mut entry_ref = entry;
                entry_ref.read_to_end(&mut content)?;
                std::fs::write(&target, &content)?;

                overrides.push(OverrideFile {
                    source_path: entry_name,
                    target_path: format!("config/{}", relative),
                });
            }
        }
    }

    progress_fn("Generic modpack imported".into(), 100, 100);

    Ok(ModpackManifest {
        name: "Imported Modpack".into(),
        mc_version: "1.20.1".into(),
        loader: "vanilla".into(),
        loader_version: String::new(),
        mods,
        overrides,
        settings: None,
    })
}

// ─── Export ────────────────────────────────────────────────────────

pub fn export_modpack(
    game_dir: &Path,
    name: &str,
    mc_version: &str,
    loader: &str,
    output_path: &Path,
) -> Result<PathBuf> {
    let file = std::fs::File::create(output_path)?;
    let mut archive = zip::ZipWriter::new(file);

    let mut index_files = Vec::new();

    // Scan mods/ directory
    let mods_dir = game_dir.join("mods");
    if mods_dir.exists() {
        for entry in std::fs::read_dir(&mods_dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                let filename = path.file_name().unwrap().to_string_lossy().to_string();
                let mod_path = format!("mods/{}", filename);

                // Add JAR to archive
                let mut content = std::fs::read(&path)?;
                use std::io::Write;
                archive.start_file(&mod_path, zip::write::FileOptions::default())?;
                archive.write_all(&mut content)?;

                index_files.push(serde_json::json!({
                    "path": mod_path,
                    "hashes": {},
                    "downloads": [],
                    "env": { "client": "required", "server": "required" }
                }));
            }
        }
    }

    // Scan config/ directory
    let config_dir = game_dir.join("config");
    if config_dir.exists() {
        add_dir_to_zip(&mut archive, &config_dir, game_dir, "overrides/config")?;
    }

    // Build modpack.index.json
    let index = serde_json::json!({
        "game": "minecraft",
        "formatVersion": 1,
        "versionId": "1.0.0",
        "name": name,
        "dependencies": {
            "minecraft": mc_version,
            if loader != "vanilla" { format!("{}-loader", loader) } else { String::new() }: "".to_string()
        },
        "files": index_files,
        "overrides": {
            "path": "overrides",
            "hashes": {}
        }
    });

    use std::io::Write;
    archive.start_file("modpack.index.json", zip::write::FileOptions::default())?;
    archive.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;

    archive.finish()?;

    Ok(output_path.to_path_buf())
}

// ─── Helpers ───────────────────────────────────────────────────────

fn detect_loader_from_mrpack(deps: Option<&serde_json::Value>) -> (String, String) {
    let deps = match deps {
        Some(d) => d,
        None => return ("vanilla".into(), String::new()),
    };

    if let Some(v) = deps.get("fabric-loader").and_then(|v| v.as_str()) {
        return ("fabric".into(), v.to_string());
    }
    if let Some(v) = deps.get("forge").and_then(|v| v.as_str()) {
        return ("forge".into(), v.to_string());
    }
    if let Some(v) = deps.get("quilt-loader").and_then(|v| v.as_str()) {
        return ("quilt".into(), v.to_string());
    }

    ("vanilla".into(), String::new())
}

fn detect_loader_from_cf(loaders: Option<&serde_json::Value>) -> (String, String) {
    let loaders = match loaders {
        Some(serde_json::Value::Array(arr)) => arr,
        _ => return ("vanilla".into(), String::new()),
    };

    if let Some(first) = loaders.first() {
        let id = first["id"].as_str().unwrap_or("");
        if id.starts_with("forge") {
            let version = id.strip_prefix("forge-").unwrap_or("").to_string();
            return ("forge".into(), version);
        }
        if id.starts_with("fabric") {
            let version = id.strip_prefix("fabric-").unwrap_or("").to_string();
            return ("fabric".into(), version);
        }
    }

    ("vanilla".into(), String::new())
}

fn add_dir_to_zip(
    archive: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    base: &Path,
    prefix: &str,
) -> Result<()> {
    use std::io::Write;
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(archive, &path, base, prefix)?;
        } else {
            let relative = path.strip_prefix(base).unwrap_or(&path);
            let zip_path = format!("{}/{}", prefix, relative.to_string_lossy());
            let mut content = std::fs::read(&path)?;
            archive.start_file(&zip_path, zip::write::FileOptions::default())?;
            archive.write_all(&mut content)?;
        }
    }
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_loader_from_mrpack() {
        let deps = serde_json::json!({
            "minecraft": "1.20.1",
            "fabric-loader": "0.15.0"
        });
        let (loader, version) = detect_loader_from_mrpack(Some(&deps));
        assert_eq!(loader, "fabric");
        assert_eq!(version, "0.15.0");
    }

    #[test]
    fn test_detect_loader_from_mrpack_forge() {
        let deps = serde_json::json!({
            "minecraft": "1.20.1",
            "forge": "47.2.0"
        });
        let (loader, version) = detect_loader_from_mrpack(Some(&deps));
        assert_eq!(loader, "forge");
        assert_eq!(version, "47.2.0");
    }

    #[test]
    fn test_detect_loader_from_cf() {
        let loaders = serde_json::json!([
            { "id": "forge-47.2.0", "primary": true }
        ]);
        let (loader, version) = detect_loader_from_cf(Some(&loaders));
        assert_eq!(loader, "forge");
        assert_eq!(version, "47.2.0");
    }

    #[test]
    fn test_modpack_manifest_serialization() {
        let manifest = ModpackManifest {
            name: "Test".into(),
            mc_version: "1.20.1".into(),
            loader: "fabric".into(),
            loader_version: "0.15.0".into(),
            mods: Vec::new(),
            overrides: Vec::new(),
            settings: None,
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: ModpackManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.loader, "fabric");
    }
}
