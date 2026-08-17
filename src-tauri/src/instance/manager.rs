use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InstanceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Instance not found: {0}")]
    NotFound(String),
    #[error("Instance already exists: {0}")]
    AlreadyExists(String),
}

pub type Result<T> = std::result::Result<T, InstanceError>;

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    #[serde(default, rename = "loaderVersion")]
    pub loader_version: Option<String>,
    #[serde(rename = "gameDir")]
    pub game_dir: PathBuf,
    pub created: String,
    #[serde(default, rename = "lastPlayed")]
    pub last_played: Option<String>,
    #[serde(default, rename = "playTimeSeconds")]
    pub play_time_seconds: u64,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub profile: InstanceProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstanceProfile {
    #[serde(default, rename = "javaPath")]
    pub java_path: Option<String>,
    #[serde(default, rename = "jvmArgs")]
    pub jvm_args: Vec<String>,
    #[serde(default, rename = "maxMemory")]
    pub max_memory: String,
    #[serde(default, rename = "minMemory")]
    pub min_memory: String,
    #[serde(default, rename = "resolutionWidth")]
    pub resolution_width: Option<u32>,
    #[serde(default, rename = "resolutionHeight")]
    pub resolution_height: Option<u32>,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default, rename = "gameArgs")]
    pub game_args: Vec<String>,
    #[serde(default, rename = "environmentVars")]
    pub environment_vars: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub profile: Option<InstanceProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub mods: u64,
    pub saves: u64,
    pub resourcepacks: u64,
    pub shaderpacks: u64,
    pub config: u64,
    pub logs: u64,
    pub total: u64,
}

fn iso_now() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}Z", t.as_secs(), t.subsec_millis())
}

fn new_uuid() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    t.as_nanos().hash(&mut hasher);
    std::ptr::hash(&hasher as *const _, &mut hasher);

    let h = hasher.finish();
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (h >> 32) as u32,
        ((h >> 16) & 0xffff) as u16,
        (h & 0xffff) as u16,
        ((h >> 48) & 0xffff) as u16,
        h & 0xffffffffffff
    )
}

// ─── Core functions ────────────────────────────────────────────────

pub fn instances_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("yorucraft")
        .join("instances")
}

fn manifest_path(instances: &Path, id: &str) -> PathBuf {
    instances.join(id).join("manifest.json")
}

pub fn list_instances() -> Result<Vec<GameInstance>> {
    let dir = instances_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut instances: Vec<GameInstance> = Vec::new();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let mp = manifest_path(&dir, &entry.file_name().to_string_lossy());
        if mp.exists() {
            let content = std::fs::read_to_string(&mp)?;
            if let Ok(inst) = serde_json::from_str::<GameInstance>(&content) {
                instances.push(inst);
            }
        }
    }

    instances.sort_by(|a, b| {
        b.last_played
            .as_deref()
            .unwrap_or("")
            .cmp(a.last_played.as_deref().unwrap_or(""))
    });

    Ok(instances)
}

pub fn create_instance(
    name: &str,
    mc_version: &str,
    loader: &str,
    loader_version: Option<&str>,
) -> Result<GameInstance> {
    let dir = instances_dir();
    let id = new_uuid();
    let game_dir = dir.join(&id);

    // Create directory structure
    let subdirs = ["mods", "config", "saves", "resourcepacks", "shaderpacks", "logs", "crash-reports"];
    for sub in &subdirs {
        std::fs::create_dir_all(game_dir.join(sub))?;
    }

    let now = iso_now();
    let instance = GameInstance {
        id: id.clone(),
        name: name.to_string(),
        mc_version: mc_version.to_string(),
        loader: loader.to_string(),
        loader_version: loader_version.map(String::from),
        game_dir: game_dir.clone(),
        created: now,
        last_played: None,
        play_time_seconds: 0,
        icon: None,
        description: None,
        profile: InstanceProfile::default(),
    };

    let mp = manifest_path(&dir, &id);
    let content = serde_json::to_string_pretty(&instance)?;
    std::fs::write(mp, content)?;

    Ok(instance)
}

pub fn delete_instance(instance_id: &str, keep_saves: bool) -> Result<()> {
    let dir = instances_dir();
    let game_dir = dir.join(instance_id);

    if !game_dir.exists() {
        return Err(InstanceError::NotFound(instance_id.to_string()));
    }

    if keep_saves {
        let saves = game_dir.join("saves");
        let saves_temp = dir.join(format!("{}_saves_temp", instance_id));

        if saves.exists() {
            std::fs::rename(&saves, &saves_temp)?;
        }

        std::fs::remove_dir_all(&game_dir)?;
        std::fs::create_dir_all(&game_dir)?;

        if saves_temp.exists() {
            std::fs::rename(&saves_temp, &saves)?;
        }
    } else {
        std::fs::remove_dir_all(&game_dir)?;
    }

    Ok(())
}

pub fn clone_instance(instance_id: &str, new_name: &str) -> Result<GameInstance> {
    let source = get_instance(instance_id)?;
    let dir = instances_dir();
    let new_id = new_uuid();
    let new_game_dir = dir.join(&new_id);

    // Copy game dir (skip logs and crash-reports)
    let source_dir = &source.game_dir;
    copy_dir_excluding(source_dir, &new_game_dir, &["logs", "crash-reports"])?;

    let now = iso_now();
    let instance = GameInstance {
        id: new_id.clone(),
        name: new_name.to_string(),
        mc_version: source.mc_version,
        loader: source.loader,
        loader_version: source.loader_version,
        game_dir: new_game_dir,
        created: now,
        last_played: None,
        play_time_seconds: 0,
        icon: source.icon,
        description: source.description,
        profile: source.profile,
    };

    let mp = manifest_path(&dir, &new_id);
    let content = serde_json::to_string_pretty(&instance)?;
    std::fs::write(mp, content)?;

    Ok(instance)
}

pub fn get_instance(instance_id: &str) -> Result<GameInstance> {
    let dir = instances_dir();
    let mp = manifest_path(&dir, instance_id);

    if !mp.exists() {
        return Err(InstanceError::NotFound(instance_id.to_string()));
    }

    let content = std::fs::read_to_string(&mp)?;
    let instance: GameInstance = serde_json::from_str(&content)?;
    Ok(instance)
}

pub fn update_instance(instance_id: &str, updates: &InstanceUpdate) -> Result<GameInstance> {
    let mut instance = get_instance(instance_id)?;

    if let Some(ref name) = updates.name {
        instance.name = name.clone();
    }
    if let Some(ref desc) = updates.description {
        instance.description = Some(desc.clone());
    }
    if let Some(ref icon) = updates.icon {
        instance.icon = Some(icon.clone());
    }
    if let Some(ref profile) = updates.profile {
        instance.profile = profile.clone();
    }

    let dir = instances_dir();
    let mp = manifest_path(&dir, instance_id);
    let content = serde_json::to_string_pretty(&instance)?;
    std::fs::write(mp, content)?;

    Ok(instance)
}

pub fn record_launch(instance_id: &str) -> Result<()> {
    let mut instance = get_instance(instance_id)?;
    instance.last_played = Some(iso_now());

    let dir = instances_dir();
    let mp = manifest_path(&dir, instance_id);
    let content = serde_json::to_string_pretty(&instance)?;
    std::fs::write(mp, content)?;
    Ok(())
}

pub fn add_play_time(instance_id: &str, seconds: u64) -> Result<()> {
    let mut instance = get_instance(instance_id)?;
    instance.play_time_seconds += seconds;

    let dir = instances_dir();
    let mp = manifest_path(&dir, instance_id);
    let content = serde_json::to_string_pretty(&instance)?;
    std::fs::write(mp, content)?;
    Ok(())
}

pub fn get_disk_usage(instance_id: &str) -> Result<DiskUsage> {
    let dir = instances_dir();
    let game_dir = dir.join(instance_id);

    if !game_dir.exists() {
        return Err(InstanceError::NotFound(instance_id.to_string()));
    }

    Ok(DiskUsage {
        mods: dir_size(&game_dir.join("mods")),
        saves: dir_size(&game_dir.join("saves")),
        resourcepacks: dir_size(&game_dir.join("resourcepacks")),
        shaderpacks: dir_size(&game_dir.join("shaderpacks")),
        config: dir_size(&game_dir.join("config")),
        logs: dir_size(&game_dir.join("logs")),
        total: dir_size(&game_dir),
    })
}

// ─── Helpers ───────────────────────────────────────────────────────

fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = std::fs::metadata(&p) {
                total += meta.len();
            }
        }
    }
    total
}

fn copy_dir_excluding(src: &Path, dst: &Path, exclude: &[&str]) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)?.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if exclude.contains(&name_str.as_ref()) {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&name);

        if src_path.is_dir() {
            copy_dir_excluding(&src_path, &dst_path, exclude)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iso_now_format() {
        let now = iso_now();
        assert!(now.ends_with('Z'));
        assert!(now.contains('-'));
        assert!(now.contains(':'));
    }

    #[test]
    fn test_new_uuid_format() {
        let id = new_uuid();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
    }

    #[test]
    fn test_disk_usage_zero_when_missing() {
        let usage = DiskUsage {
            mods: 0,
            saves: 0,
            resourcepacks: 0,
            shaderpacks: 0,
            config: 0,
            logs: 0,
            total: 0,
        };
        assert_eq!(usage.total, 0);
    }

    #[test]
    fn test_instance_profile_defaults() {
        let profile = InstanceProfile::default();
        assert!(profile.jvm_args.is_empty());
        assert!(profile.game_args.is_empty());
        assert!(!profile.fullscreen);
        assert!(profile.java_path.is_none());
    }

    #[test]
    fn test_instance_serialization() {
        let instance = GameInstance {
            id: "test-id".into(),
            name: "Test".into(),
            mc_version: "1.21".into(),
            loader: "vanilla".into(),
            loader_version: None,
            game_dir: PathBuf::from("/tmp/test"),
            created: "2024-01-01T00:00:00Z".into(),
            last_played: None,
            play_time_seconds: 0,
            icon: None,
            description: None,
            profile: InstanceProfile::default(),
        };
        let json = serde_json::to_string(&instance).unwrap();
        let parsed: GameInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.id, "test-id");
    }
}
