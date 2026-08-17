use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::launcher::java;

#[derive(Error, Debug)]
pub enum PreflightError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, PreflightError>;

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub fix_available: bool,
    pub fix_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightReport {
    pub checks: Vec<PreflightCheck>,
    pub overall: CheckStatus,
    pub can_launch: bool,
    pub recommendations: Vec<String>,
}

// ─── Entry point ───────────────────────────────────────────────────

pub fn run_all_checks(
    version_id: &str,
    game_dir: &Path,
    username: &str,
) -> PreflightReport {
    let mut checks = Vec::new();

    checks.push(check_username(username));
    checks.push(check_java(version_id));
    checks.push(check_disk_space(game_dir, version_id));
    checks.push(check_natives(game_dir));
    checks.push(check_username_length(username));

    let has_fail = checks.iter().any(|c| matches!(c.status, CheckStatus::Fail));
    let has_warn = checks.iter().any(|c| matches!(c.status, CheckStatus::Warn));

    let overall = if has_fail {
        CheckStatus::Fail
    } else if has_warn {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };

    let mut recommendations = Vec::new();
    if has_fail {
        recommendations.push("Fix all failing checks before launching".into());
    }
    if has_warn {
        recommendations.push("Warnings detected — launch may still work but could be unstable".into());
    }

    PreflightReport {
        checks,
        overall,
        can_launch: !has_fail,
        recommendations,
    }
}

// ─── Individual checks ─────────────────────────────────────────────

fn check_java(version_id: &str) -> PreflightCheck {
    let required = required_java_for_version(version_id);
    match java::find_java().into_iter().find(|j| j.major_version >= required && j.is_valid) {
        Some(inst) => PreflightCheck {
            name: "Java Version".into(),
            status: CheckStatus::Pass,
            message: format!("Java {} found", inst.version),
            fix_available: false,
            fix_action: None,
        },
        None => PreflightCheck {
            name: "Java Version".into(),
            status: CheckStatus::Fail,
            message: format!("No Java {}+ installation found (required for {})", required, version_id),
            fix_available: true,
            fix_action: Some("download_java".into()),
        },
    }
}

fn check_disk_space(game_dir: &Path, _version_id: &str) -> PreflightCheck {
    match available_space(game_dir) {
        Some(mb) => {
            let needed_mb: u64 = 800;
            if mb >= needed_mb {
                PreflightCheck {
                    name: "Disk Space".into(),
                    status: CheckStatus::Pass,
                    message: format!("{}GB free", mb / 1024),
                    fix_available: false,
                    fix_action: None,
                }
            } else if mb >= 200 {
                PreflightCheck {
                    name: "Disk Space".into(),
                    status: CheckStatus::Warn,
                    message: format!("Low disk space — {}MB free, ~{}MB required", mb, needed_mb),
                    fix_available: false,
                    fix_action: None,
                }
            } else {
                PreflightCheck {
                    name: "Disk Space".into(),
                    status: CheckStatus::Fail,
                    message: format!("Insufficient disk space — {}MB free", mb),
                    fix_available: false,
                    fix_action: None,
                }
            }
        }
        None => PreflightCheck {
            name: "Disk Space".into(),
            status: CheckStatus::Warn,
            message: "Could not determine available disk space".into(),
            fix_available: false,
            fix_action: None,
        },
    }
}

fn check_natives(game_dir: &Path) -> PreflightCheck {
    let natives_dir = game_dir.join("natives");
    if !natives_dir.exists() {
        return PreflightCheck {
            name: "Native Libraries".into(),
            status: CheckStatus::Warn,
            message: "Natives directory not found — will extract on launch".into(),
            fix_available: false,
            fix_action: None,
        };
    }

    let count = count_native_files(&natives_dir);
    if count > 0 {
        PreflightCheck {
            name: "Native Libraries".into(),
            status: CheckStatus::Pass,
            message: format!("{} native files present", count),
            fix_available: false,
            fix_action: None,
        }
    } else {
        PreflightCheck {
            name: "Native Libraries".into(),
            status: CheckStatus::Warn,
            message: "Natives directory exists but is empty".into(),
            fix_available: true,
            fix_action: Some("reextract_natives".into()),
        }
    }
}

fn check_username(username: &str) -> PreflightCheck {
    if username.is_empty() {
        return PreflightCheck {
            name: "Username".into(),
            status: CheckStatus::Fail,
            message: "Username is empty".into(),
            fix_available: false,
            fix_action: None,
        };
    }
    PreflightCheck {
        name: "Username".into(),
        status: CheckStatus::Pass,
        message: format!("Username '{}' is valid", username),
        fix_available: false,
        fix_action: None,
    }
}

fn check_username_length(username: &str) -> PreflightCheck {
    let len = username.len();
    if len < 3 {
        PreflightCheck {
            name: "Username Length".into(),
            status: CheckStatus::Fail,
            message: format!("Username too short — {} chars, minimum is 3", len),
            fix_available: false,
            fix_action: None,
        }
    } else if len > 16 {
        PreflightCheck {
            name: "Username Length".into(),
            status: CheckStatus::Fail,
            message: format!("Username too long — {} chars, maximum is 16", len),
            fix_available: false,
            fix_action: None,
        }
    } else {
        PreflightCheck {
            name: "Username Length".into(),
            status: CheckStatus::Pass,
            message: format!("{} characters", len),
            fix_available: false,
            fix_action: None,
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────

fn required_java_for_version(version_id: &str) -> u32 {
    let parts: Vec<&str> = version_id.split('.').collect();
    if parts.len() < 2 {
        return 21;
    }
    let minor: u32 = parts[1].parse().unwrap_or(0);
    match minor {
        8..=16 => 8,
        17 => 16,
        18..=20 => 17,
        _ => 21,
    }
}

fn available_space(path: &Path) -> Option<u64> {
    // Use statvfs on Unix systems
    #[cfg(unix)]
    {
        use std::ffi::CString;
        let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        unsafe {
            if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
                let free = stat.f_bavail as u64 * stat.f_frsize as u64;
                return Some(free / (1024 * 1024));
            }
        }
        return None;
    }

    // On non-unix, we can't easily get disk space without extra deps
    #[cfg(not(unix))]
    {
        None
    }
}

fn count_native_files(natives_dir: &Path) -> usize {
    let native_exts = ["dll", "so", "dylib"];
    std::fs::read_dir(natives_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map_or(false, |ext| native_exts.contains(&ext))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_username_empty() {
        let check = check_username("");
        assert!(matches!(check.status, CheckStatus::Fail));
    }

    #[test]
    fn test_check_username_valid() {
        let check = check_username("Steve");
        assert!(matches!(check.status, CheckStatus::Pass));
    }

    #[test]
    fn test_check_username_length_short() {
        let check = check_username_length("ab");
        assert!(matches!(check.status, CheckStatus::Fail));
    }

    #[test]
    fn test_check_username_length_ok() {
        let check = check_username_length("Notch");
        assert!(matches!(check.status, CheckStatus::Pass));
    }

    #[test]
    fn test_check_username_length_long() {
        let name = "a".repeat(20);
        let check = check_username_length(&name);
        assert!(matches!(check.status, CheckStatus::Fail));
    }

    #[test]
    fn test_required_java_for_version() {
        assert_eq!(required_java_for_version("1.8.9"), 8);
        assert_eq!(required_java_for_version("1.16.5"), 8);
        assert_eq!(required_java_for_version("1.17.1"), 16);
        assert_eq!(required_java_for_version("1.20.4"), 17);
        assert_eq!(required_java_for_version("1.21.4"), 21);
    }

    #[test]
    fn test_count_native_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(count_native_files(dir.path()), 0);
    }

    #[test]
    fn test_count_native_files_with_jars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lib.jar"), b"").unwrap();
        assert_eq!(count_native_files(dir.path()), 0);
    }

    #[test]
    fn test_count_native_files_with_natives() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("liblwjgl.so"), b"").unwrap();
        std::fs::write(dir.path().join("libgl.so"), b"").unwrap();
        assert_eq!(count_native_files(dir.path()), 2);
    }
}
