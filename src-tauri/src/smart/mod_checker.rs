use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Error, Debug)]
pub enum ModCheckError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, ModCheckError>;

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub mc_versions: Vec<String>,
    pub dependencies: Vec<ModDependency>,
    pub conflicts: Vec<String>,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependency {
    pub mod_id: String,
    pub required: bool,
    pub version_range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictReason {
    DirectConflict(String),
    DuplicateMod(String),
    IncompatibleVersions(String),
    MissingDependency(String),
    VersionMismatch(String),
}

impl std::fmt::Display for ConflictReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DirectConflict(s) => write!(f, "{}", s),
            Self::DuplicateMod(s) => write!(f, "Duplicate mod: {}", s),
            Self::IncompatibleVersions(s) => write!(f, "{}", s),
            Self::MissingDependency(s) => write!(f, "Missing dependency: {}", s),
            Self::VersionMismatch(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModConflict {
    pub mod_a: String,
    pub mod_b: String,
    pub reason: ConflictReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModCheckResult {
    pub mods_found: Vec<ModInfo>,
    pub conflicts: Vec<ModConflict>,
    pub missing_dependencies: Vec<(String, String)>,
    pub warnings: Vec<String>,
    pub can_launch: bool,
}

// ─── Known conflict database ───────────────────────────────────────

fn known_conflicts() -> HashMap<&'static str, Vec<&'static str>> {
    let mut map = HashMap::new();
    map.insert("optifine", vec!["sodium", "lithium", "starlight", "embeddium"]);
    map.insert("sodium", vec!["rubidium", "optifine", "embeddium"]);
    map.insert("forge", vec!["fabric-api"]);
    map.insert("morechat", vec!["betterchat"]);
    map
}

// ─── Scanning ──────────────────────────────────────────────────────

pub fn scan_mods(mods_dir: &Path) -> Result<Vec<ModInfo>> {
    if !mods_dir.exists() {
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    for entry in std::fs::read_dir(mods_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jar") {
            continue;
        }
        if let Ok(Some(info)) = read_mod_from_jar(&path) {
            mods.push(info);
        }
    }

    mods.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(mods)
}

fn read_mod_from_jar(jar_path: &Path) -> Result<Option<ModInfo>> {
    let file = std::fs::File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)?;

    if let Ok(mut file) = archive.by_name("fabric.mod.json") {
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        if let Ok(info) = parse_fabric_mod(&contents, jar_path) {
            return Ok(Some(info));
        }
    }

    if let Ok(mut file) = archive.by_name("META-INF/mods.toml") {
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        if let Ok(info) = parse_forge_toml(&contents, jar_path) {
            return Ok(Some(info));
        }
    }

    if let Ok(mut file) = archive.by_name("mcmod.info") {
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        if let Ok(info) = parse_legacy_forge(&contents, jar_path) {
            return Ok(Some(info));
        }
    }

    Ok(None)
}

fn parse_fabric_mod(json: &str, jar_path: &Path) -> Result<ModInfo> {
    let v: serde_json::Value = serde_json::from_str(json)?;

    let id = v["id"].as_str().unwrap_or("unknown").to_string();
    let name = v["name"].as_str().unwrap_or(&id).to_string();
    let version = v["version"].as_str().unwrap_or("0.0.0").to_string();

        let mc_versions = v
            .get("depends")
            .and_then(|d| d.get("minecraft"))
            .and_then(|m| m.as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();

    let mut conflicts = Vec::new();
    if let Some(suggests) = v.get("suggests") {
        // Conflicts in Fabric are declared via "conflicts" or "recommends" (inverted)
        if let Some(obj) = suggests.as_object() {
            for key in obj.keys() {
                conflicts.push(key.clone());
            }
        }
    }

    let dependencies = parse_fabric_deps(&v);

    Ok(ModInfo {
        id,
        name,
        version,
        mc_versions,
        dependencies,
        conflicts,
        file_path: jar_path.to_path_buf(),
    })
}

fn parse_fabric_deps(v: &serde_json::Value) -> Vec<ModDependency> {
    let mut deps = Vec::new();

    for dep_key in &["depends", "recommends"] {
        if let Some(obj) = v.get(*dep_key).and_then(|d| d.as_object()) {
            for (mod_id, val) in obj {
                if mod_id == "minecraft" || mod_id == "java" || mod_id == "fabricloader" {
                    continue;
                }
                let (required, range) = match val {
                    serde_json::Value::String(s) => (dep_key == &"depends", Some(s.clone())),
                    serde_json::Value::Bool(b) => (*b, None),
                    _ => (false, None),
                };
                deps.push(ModDependency {
                    mod_id: mod_id.clone(),
                    required,
                    version_range: range,
                });
            }
        }
    }

    deps
}

fn parse_forge_toml(toml: &str, jar_path: &Path) -> Result<ModInfo> {
    let mut id = String::new();
    let mut name = String::new();
    let mut version = String::new();
    let mut mc_versions = Vec::new();
    let mut in_mod = false;

    for line in toml.lines() {
        let trimmed = line.trim();

        if trimmed == "[[mods]]" {
            in_mod = true;
            continue;
        }
        if trimmed.starts_with('[') && !trimmed.starts_with("[[") {
            in_mod = false;
            continue;
        }

        if in_mod {
            if let Some(val) = trimmed.strip_prefix("modId").and_then(|s| s.strip_prefix('=')) {
                id = val.trim().trim_matches('"').to_string();
            } else if let Some(val) = trimmed.strip_prefix("displayName").and_then(|s| s.strip_prefix('=')) {
                name = val.trim().trim_matches('"').to_string();
            } else if let Some(val) = trimmed.strip_prefix("version").and_then(|s| s.strip_prefix('=')) {
                version = val.trim().trim_matches('"').to_string();
            }
        }

        if trimmed.starts_with("loaderVersion") || trimmed.starts_with("mcVersion") {
            if let Some(val) = trimmed.split('=').nth(1) {
                mc_versions.push(val.trim().trim_matches('"').to_string());
            }
        }
    }

    if id.is_empty() {
        id = "unknown-forge-mod".to_string();
    }
    if name.is_empty() {
        name = id.clone();
    }

    Ok(ModInfo {
        id,
        name,
        version: if version.is_empty() { "0.0.0".into() } else { version },
        mc_versions,
        dependencies: Vec::new(),
        conflicts: Vec::new(),
        file_path: jar_path.to_path_buf(),
    })
}

fn parse_legacy_forge(json: &str, jar_path: &Path) -> Result<ModInfo> {
    let v: serde_json::Value = serde_json::from_str(json)?;

    let obj = v
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_object());

    if let Some(obj) = obj {
        let id = obj.get("modid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
        let version = obj.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string();
        let mc_versions = obj
            .get("mcversion")
            .and_then(|v| v.as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();

        Ok(ModInfo {
            id,
            name,
            version,
            mc_versions,
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            file_path: jar_path.to_path_buf(),
        })
    } else {
        Ok(ModInfo {
            id: "unknown".into(),
            name: "Unknown Mod".into(),
            version: "0.0.0".into(),
            mc_versions: Vec::new(),
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            file_path: jar_path.to_path_buf(),
        })
    }
}

// ─── Conflict detection ────────────────────────────────────────────

pub fn check_conflicts(mods: &[ModInfo]) -> Vec<ModConflict> {
    let mut conflicts = Vec::new();
    let known = known_conflicts();

    // Duplicate detection
    let mut seen: HashMap<&str, &ModInfo> = HashMap::new();
    for m in mods {
        if let Some(prev) = seen.get(m.id.as_str()) {
            conflicts.push(ModConflict {
                mod_a: prev.name.clone(),
                mod_b: m.name.clone(),
                reason: ConflictReason::DuplicateMod(m.id.clone()),
            });
        }
        seen.insert(&m.id, m);
    }

    // Known conflict database
    for m in mods {
        if let Some(known_list) = known.get(m.id.as_str()) {
            for other in mods {
                if other.id == m.id {
                    continue;
                }
                if known_list.iter().any(|k| *k == other.id.as_str()) {
                    let already = conflicts.iter().any(|c| {
                        (c.mod_a == m.name && c.mod_b == other.name)
                            || (c.mod_a == other.name && c.mod_b == m.name)
                    });
                    if !already {
                        conflicts.push(ModConflict {
                            mod_a: m.name.clone(),
                            mod_b: other.name.clone(),
                            reason: ConflictReason::DirectConflict(format!(
                                "{} and {} are known to be incompatible",
                                m.name, other.name
                            )),
                        });
                    }
                }
            }
        }
    }

    // Declared conflicts
    for m in mods {
        for conflict_id in &m.conflicts {
            if let Some(other) = mods.iter().find(|o| o.id == *conflict_id) {
                let already = conflicts.iter().any(|c| {
                    (c.mod_a == m.name && c.mod_b == other.name)
                        || (c.mod_a == other.name && c.mod_b == m.name)
                });
                if !already {
                    conflicts.push(ModConflict {
                        mod_a: m.name.clone(),
                        mod_b: other.name.clone(),
                        reason: ConflictReason::DirectConflict(
                            "declared conflict in mod metadata".into(),
                        ),
                    });
                }
            }
        }
    }

    conflicts
}

pub fn check_dependencies(mods: &[ModInfo]) -> Vec<ModConflict> {
    let installed: HashSet<&str> = mods.iter().map(|m| m.id.as_str()).collect();
    let mut issues = Vec::new();

    for m in mods {
        for dep in &m.dependencies {
            if dep.required && !installed.contains(dep.mod_id.as_str()) {
                issues.push(ModConflict {
                    mod_a: m.name.clone(),
                    mod_b: dep.mod_id.clone(),
                    reason: ConflictReason::MissingDependency(dep.mod_id.clone()),
                });
            }
        }
    }

    issues
}

fn version_matches(mod_version: &str, mc_version: &str) -> bool {
    if mod_version == "*" {
        return true;
    }
    if mod_version == mc_version {
        return true;
    }
    if mod_version.ends_with(".x") || mod_version.ends_with(".*") {
        let prefix = mod_version.trim_end_matches('x').trim_end_matches('.').trim_end_matches('*').trim_end_matches('.');
        return mc_version.starts_with(prefix);
    }
    false
}

pub fn check_version_compatibility(mods: &[ModInfo], mc_version: &str) -> Vec<ModConflict> {
    let mut issues = Vec::new();

    for m in mods {
        if m.mc_versions.is_empty() {
            continue;
        }
        let compatible = m.mc_versions.iter().any(|v| version_matches(v, mc_version));
        if !compatible {
            issues.push(ModConflict {
                mod_a: m.name.clone(),
                mod_b: mc_version.to_string(),
                reason: ConflictReason::IncompatibleVersions(format!(
                    "{} supports [{}], but running {}",
                    m.name,
                    m.mc_versions.join(", "),
                    mc_version
                )),
            });
        }
    }

    issues
}

// ─── Full check ────────────────────────────────────────────────────

pub fn full_check(mods_dir: &Path, mc_version: &str) -> ModCheckResult {
    let mods = scan_mods(mods_dir).unwrap_or_default();
    let mut conflicts = check_conflicts(&mods);
    let dep_issues = check_dependencies(&mods);
    let version_issues = check_version_compatibility(&mods, mc_version);

    let warnings: Vec<String> = dep_issues.iter()
        .map(|i| format!("Missing dependency: {}", i.mod_b))
        .chain(version_issues.iter().map(|i| i.reason.to_string()))
        .collect();

    let missing_dependencies = dep_issues.iter()
        .map(|i| (i.mod_a.clone(), i.mod_b.clone()))
        .collect();

    conflicts.extend(dep_issues);
    conflicts.extend(version_issues);

    let has_critical = conflicts.iter().any(|c| {
        matches!(
            c.reason,
            ConflictReason::DuplicateMod(_) | ConflictReason::DirectConflict(_)
        )
    });

    ModCheckResult {
        mods_found: mods,
        conflicts,
        missing_dependencies,
        warnings,
        can_launch: !has_critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mod(id: &str, name: &str, mc_versions: Vec<&str>) -> ModInfo {
        ModInfo {
            id: id.into(),
            name: name.into(),
            version: "1.0.0".into(),
            mc_versions: mc_versions.into_iter().map(String::from).collect(),
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            file_path: PathBuf::from("/dev/null"),
        }
    }

    #[test]
    fn test_no_conflicts_empty() {
        assert!(check_conflicts(&[]).is_empty());
    }

    #[test]
    fn test_duplicate_mod() {
        let mods = vec![
            make_mod("sodium", "Sodium", vec!["1.20.4"]),
            make_mod("sodium", "Sodium", vec!["1.20.4"]),
        ];
        let conflicts = check_conflicts(&mods);
        assert!(conflicts.iter().any(|c| matches!(c.reason, ConflictReason::DuplicateMod(_))));
    }

    #[test]
    fn test_known_conflict_optifine_sodium() {
        let mods = vec![
            make_mod("optifine", "OptiFine", vec!["1.20.4"]),
            make_mod("sodium", "Sodium", vec!["1.20.4"]),
        ];
        let conflicts = check_conflicts(&mods);
        assert!(conflicts.iter().any(|c| matches!(c.reason, ConflictReason::DirectConflict(_))));
    }

    #[test]
    fn test_missing_dependency() {
        let mut m = make_mod("modA", "Mod A", vec!["1.20.4"]);
        m.dependencies.push(ModDependency {
            mod_id: "modB".into(),
            required: true,
            version_range: None,
        });
        let mods = vec![m];
        let issues = check_dependencies(&mods);
        assert_eq!(issues.len(), 1);
        assert!(matches!(issues[0].reason, ConflictReason::MissingDependency(_)));
    }

    #[test]
    fn test_version_match_exact() {
        assert!(version_matches("1.20.4", "1.20.4"));
        assert!(!version_matches("1.20.4", "1.20.5"));
    }

    #[test]
    fn test_version_match_wildcard() {
        assert!(version_matches("1.20.x", "1.20.4"));
        assert!(version_matches("1.20.*", "1.20.1"));
        assert!(!version_matches("1.20.x", "1.21.0"));
    }

    #[test]
    fn test_version_match_star() {
        assert!(version_matches("*", "1.20.4"));
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mods = scan_mods(dir.path()).unwrap();
        assert!(mods.is_empty());
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let path = Path::new("/nonexistent/mods/dir");
        let mods = scan_mods(path).unwrap();
        assert!(mods.is_empty());
    }

    #[test]
    fn test_full_check_no_mods() {
        let dir = tempfile::tempdir().unwrap();
        let result = full_check(dir.path(), "1.20.4");
        assert!(result.can_launch);
        assert!(result.conflicts.is_empty());
    }
}
