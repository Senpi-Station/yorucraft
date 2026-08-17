use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JavaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No Java installation found")]
    NotFound,
    #[error("Failed to verify Java: {0}")]
    VerifyFailed(String),
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Failed to deserialize Java runtime manifest: {0}")]
    Deserialize(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, JavaError>;

#[derive(Debug, Clone)]
pub struct JavaInstallation {
    pub path: PathBuf,
    pub version: String,
    pub major_version: u32,
    pub is_valid: bool,
}

pub fn find_java() -> Vec<JavaInstallation> {
    let mut installations = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(java_home)
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
        if !seen.contains(&path) {
            if let Some(inst) = verify_java(&path) {
                seen.insert(path.clone());
                installations.push(inst);
            }
        }
    }

    if let Ok(output) = Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(if cfg!(windows) { "java.exe" } else { "java" })
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let path = PathBuf::from(line.trim());
            if !seen.contains(&path) {
                if let Some(inst) = verify_java(&path) {
                    seen.insert(path.clone());
                    installations.push(inst);
                }
            }
        }
    }

    let common_dirs: Vec<PathBuf> = if cfg!(windows) {
        vec![
            PathBuf::from("C:\\Program Files\\Java"),
            PathBuf::from("C:\\Program Files (x86)\\Java"),
            home_dir().join(".jdks"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/Library/Java/JavaVirtualMachines"),
            home_dir().join("Library/Java/JavaVirtualMachines"),
        ]
    } else {
        vec![PathBuf::from("/usr/lib/jvm"), PathBuf::from("/usr/java")]
    };

    for dir in &common_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let java_path = if cfg!(windows) {
                    entry.path().join("bin\\javaw.exe")
                } else if cfg!(target_os = "macos") {
                    entry
                        .path()
                        .join("Contents/Home/bin/java")
                } else {
                    entry.path().join("bin/java")
                };
                if !seen.contains(&java_path) {
                    if let Some(inst) = verify_java(&java_path) {
                        seen.insert(java_path.clone());
                        installations.push(inst);
                    }
                }
            }
        }
    }

    installations.sort_by(|a, b| b.major_version.cmp(&a.major_version));
    installations
}

pub fn find_java_for_version(mc_version: &str) -> Option<JavaInstallation> {
    let required = required_java_version(mc_version);
    find_java()
        .into_iter()
        .find(|j| j.major_version >= required && j.is_valid)
}

pub fn verify_java(path: &Path) -> Option<JavaInstallation> {
    let output = Command::new(path).arg("-version").output().ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let version_line = stderr.lines().next()?;
    let version = parse_java_version(version_line)?;
    let major = parse_major_version(&version);
    Some(JavaInstallation {
        path: path.to_path_buf(),
        version,
        major_version: major,
        is_valid: major >= 8,
    })
}

fn parse_java_version(line: &str) -> Option<String> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

fn parse_major_version(version: &str) -> u32 {
    let major_str = version.split('.').next().unwrap_or("0");
    if major_str == "1" {
        version
            .split('_')
            .next()
            .and_then(|v| v.split('.').nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(8)
    } else {
        major_str.parse().unwrap_or(0)
    }
}

fn required_java_version(mc_version: &str) -> u32 {
    let parts: Vec<&str> = mc_version.split('.').collect();
    if parts.len() < 2 {
        return 21;
    }
    let major: u32 = parts[0].parse().unwrap_or(0);
    let minor: u32 = parts[1].parse().unwrap_or(0);

    match major {
        1 => match minor {
            8..=16 => 8,
            17 => 16,
            18..=20 => 17,
            _ => 21,
        },
        _ => 21,
    }
}

pub async fn download_mojang_java(game_dir: &Path, mc_version: &str) -> Result<PathBuf> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12879/all.json")
        .send()
        .await?;
    let manifest: serde_json::Value = resp.json().await?;

    let component = match required_java_version(mc_version) {
        8 => "java-runtime-legacy",
        16 => "java-runtime-alpha",
        17 => "java-runtime-gamma",
        _ => "java-runtime-delta",
    };

    let platform = if cfg!(target_os = "linux") {
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
        return Err(JavaError::NotFound);
    };

    let runtime_dir = game_dir.join("runtime").join("java");
    std::fs::create_dir_all(&runtime_dir)?;

    if let Some(platform_data) = manifest.get(component).and_then(|c| c.get(platform)) {
        if let Some(files) = platform_data.get("files") {
            if let Some(obj) = files.as_object() {
                for (path, info) in obj {
                    if let Some(url) = info.get("url").and_then(|u| u.as_str()) {
                        let target = runtime_dir.join(path);
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        if info.get("type").and_then(|t| t.as_str()) == Some("file") {
                            if let Ok(resp) = client.get(url).send().await {
                                if let Ok(bytes) = resp.bytes().await {
                                    let _ = std::fs::write(&target, bytes);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let java_path = if cfg!(windows) {
        runtime_dir.join("bin\\javaw.exe")
    } else {
        runtime_dir.join("bin/java")
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&java_path, std::fs::Permissions::from_mode(0o755));
    }

    if java_path.exists() {
        Ok(java_path)
    } else {
        Err(JavaError::NotFound)
    }
}

pub async fn auto_select_java(mc_version: &str, game_dir: &Path) -> Result<PathBuf> {
    if let Some(inst) = find_java_for_version(mc_version) {
        return Ok(inst.path);
    }
    download_mojang_java(game_dir, mc_version).await
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_java_version() {
        assert_eq!(
            parse_java_version("openjdk version \"21.0.2\" 2024-01-16"),
            Some("21.0.2".to_string())
        );
        assert_eq!(
            parse_java_version("java version \"1.8.0_361\""),
            Some("1.8.0_361".to_string())
        );
    }

    #[test]
    fn test_parse_major_version() {
        assert_eq!(parse_major_version("21.0.2"), 21);
        assert_eq!(parse_major_version("1.8.0_361"), 8);
        assert_eq!(parse_major_version("17.0.1"), 17);
    }

    #[test]
    fn test_required_java_version() {
        assert_eq!(required_java_version("1.8.9"), 8);
        assert_eq!(required_java_version("1.16.5"), 8);
        assert_eq!(required_java_version("1.17.1"), 16);
        assert_eq!(required_java_version("1.18.2"), 17);
        assert_eq!(required_java_version("1.20.4"), 17);
        assert_eq!(required_java_version("1.21.4"), 21);
    }
}
