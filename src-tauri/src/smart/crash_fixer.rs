use std::path::Path;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CrashFixerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CrashFixerError>;

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrashSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for CrashSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    ExceptionType(String),
    LogLine(String),
    CausedBy(String),
    Combo(Vec<DetectionMethod>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FixAction {
    Suggest(String),
    DeleteDirectory(String),
    ReinstallVersion,
    UpdateJvmArgs(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashPattern {
    pub id: String,
    pub name: String,
    pub detection: DetectionMethod,
    pub severity: CrashSeverity,
    pub description: String,
    pub fix: FixAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixSuggestion {
    pub action: FixAction,
    pub description: String,
    pub auto_applicable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashAnalysis {
    pub patterns_matched: Vec<CrashPattern>,
    pub root_cause: String,
    pub fix_suggestions: Vec<FixSuggestion>,
    pub auto_fixable: bool,
    pub confidence: f32,
}

// ─── Pattern database ──────────────────────────────────────────────

static PATTERN_DB: Lazy<Vec<CrashPattern>> = Lazy::new(build_pattern_database);

fn build_pattern_database() -> Vec<CrashPattern> {
    vec![
        // ── Java version issues ───────────────────────────
        CrashPattern {
            id: "java-unsupported-class".into(),
            name: "Unsupported Class Version".into(),
            detection: DetectionMethod::ExceptionType("UnsupportedClassVersionError".into()),
            severity: CrashSeverity::Critical,
            description: "Wrong Java version — the compiled class requires a newer JVM".into(),
            fix: FixAction::Suggest(
                "Wrong Java version. Install the required Java version from Settings → Java".into(),
            ),
        },
        CrashPattern {
            id: "java-noclassdef".into(),
            name: "Missing Java Class".into(),
            detection: DetectionMethod::LogLine(
                "NoClassDefFoundError: java/lang/invoke/MethodHandles".into(),
            ),
            severity: CrashSeverity::Critical,
            description: "Java version too new for this Minecraft version".into(),
            fix: FixAction::Suggest(
                "Java 8 is required for this Minecraft version. Your current Java is too new.".into(),
            ),
        },
        // ── Memory issues ─────────────────────────────────
        CrashPattern {
            id: "oom-heap".into(),
            name: "Out of Memory — Heap".into(),
            detection: DetectionMethod::ExceptionType("OutOfMemoryError".into()),
            severity: CrashSeverity::Critical,
            description: "Java heap space exhausted".into(),
            fix: FixAction::Suggest(
                "Increase -Xmx in launcher settings. Use the Auto-Tuner for a recommended value."
                    .into(),
            ),
        },
        CrashPattern {
            id: "oom-metaspace".into(),
            name: "Out of Memory — Metaspace".into(),
            detection: DetectionMethod::LogLine("OutOfMemoryError: Metaspace".into()),
            severity: CrashSeverity::High,
            description: "Metaspace exhausted — too many mods or classes loaded".into(),
            fix: FixAction::UpdateJvmArgs(vec!["-XX:MaxMetaspaceSize=512M".into()]),
        },
        CrashPattern {
            id: "gc-overhead".into(),
            name: "GC Overhead Limit".into(),
            detection: DetectionMethod::LogLine("GC overhead limit exceeded".into()),
            severity: CrashSeverity::Critical,
            description: "JVM spent too much time garbage collecting".into(),
            fix: FixAction::Suggest(
                "Memory is critically low. Close other applications or increase -Xmx.".into(),
            ),
        },
        // ── Native library issues ─────────────────────────
        CrashPattern {
            id: "natives-lwjgl".into(),
            name: "LWJGL Native Error".into(),
            detection: DetectionMethod::Combo(vec![
                DetectionMethod::ExceptionType("UnsatisfiedLinkError".into()),
                DetectionMethod::LogLine("lwjgl".into()),
            ]),
            severity: CrashSeverity::High,
            description: "LWJGL native libraries are corrupted or missing".into(),
            fix: FixAction::DeleteDirectory("natives".into()),
        },
        CrashPattern {
            id: "natives-glfw".into(),
            name: "GLFW Missing".into(),
            detection: DetectionMethod::LogLine("Couldn't find GLFW".into()),
            severity: CrashSeverity::High,
            description: "GLFW library not found in natives".into(),
            fix: FixAction::DeleteDirectory("natives".into()),
        },
        CrashPattern {
            id: "opengl-missing".into(),
            name: "Missing OpenGL".into(),
            detection: DetectionMethod::Combo(vec![
                DetectionMethod::ExceptionType("UnsatisfiedLinkError".into()),
                DetectionMethod::LogLine("libGL".into()),
            ]),
            severity: CrashSeverity::High,
            description: "OpenGL drivers are not installed or not compatible".into(),
            fix: FixAction::Suggest(
                "Missing OpenGL drivers. Install or update your GPU drivers.".into(),
            ),
        },
        // ── Mod issues ────────────────────────────────────
        CrashPattern {
            id: "mixin-conflict".into(),
            name: "Mixin Conflict".into(),
            detection: DetectionMethod::LogLine("MixinTransformerError".into()),
            severity: CrashSeverity::High,
            description: "Mixin transformation failed — mod conflict detected".into(),
            fix: FixAction::Suggest(
                "Mixin conflict between mods. Disable recently added mods one by one.".into(),
            ),
        },
        CrashPattern {
            id: "duplicate-mod".into(),
            name: "Duplicate Mod".into(),
            detection: DetectionMethod::LogLine("Duplicate mod".into()),
            severity: CrashSeverity::Medium,
            description: "The same mod appears multiple times in mods/".into(),
            fix: FixAction::Suggest(
                "Duplicate mod detected. Check the mods/ folder for duplicate JARs.".into(),
            ),
        },
        CrashPattern {
            id: "mod-requires-forge".into(),
            name: "Mod Requires Forge".into(),
            detection: DetectionMethod::LogLine("Mod requires Forge".into()),
            severity: CrashSeverity::High,
            description: "A mod requires Forge but it is not installed".into(),
            fix: FixAction::Suggest(
                "Install the correct Forge version for this mod.".into(),
            ),
        },
        // ── Network / login ───────────────────────────────
        CrashPattern {
            id: "invalid-session".into(),
            name: "Invalid Session".into(),
            detection: DetectionMethod::LogLine("Invalid session".into()),
            severity: CrashSeverity::Medium,
            description: "Authentication session has expired".into(),
            fix: FixAction::Suggest(
                "Session expired. Re-authenticate in Settings → Accounts.".into(),
            ),
        },
        CrashPattern {
            id: "auth-down".into(),
            name: "Authentication Servers Down".into(),
            detection: DetectionMethod::LogLine("Authentication servers are down".into()),
            severity: CrashSeverity::Medium,
            description: "Mojang authentication servers are unavailable".into(),
            fix: FixAction::Suggest(
                "Mojang authentication servers are currently unavailable. Try again later.".into(),
            ),
        },
        // ── File system issues ────────────────────────────
        CrashPattern {
            id: "asset-missing".into(),
            name: "Missing Asset File".into(),
            detection: DetectionMethod::Combo(vec![
                DetectionMethod::ExceptionType("FileNotFoundException".into()),
                DetectionMethod::LogLine("assets/objects".into()),
            ]),
            severity: CrashSeverity::High,
            description: "Asset files are missing from the cache".into(),
            fix: FixAction::ReinstallVersion,
        },
        CrashPattern {
            id: "permission-denied".into(),
            name: "Permission Denied".into(),
            detection: DetectionMethod::LogLine("Permission denied".into()),
            severity: CrashSeverity::High,
            description: "Insufficient file system permissions".into(),
            fix: FixAction::Suggest(
                "Insufficient file permissions. Check folder ownership or run with appropriate user."
                    .into(),
            ),
        },
        CrashPattern {
            id: "zip-corrupt".into(),
            name: "Corrupted ZIP/JAR".into(),
            detection: DetectionMethod::ExceptionType("ZipException".into()),
            severity: CrashSeverity::High,
            description: "A game JAR or ZIP file is corrupted".into(),
            fix: FixAction::ReinstallVersion,
        },
        // ── Platform-specific ─────────────────────────────
        CrashPattern {
            id: "macos-main-class".into(),
            name: "macOS Main Class Not Found".into(),
            detection: DetectionMethod::Combo(vec![
                DetectionMethod::LogLine("Could not find or load main class".into()),
                DetectionMethod::LogLine("mac".into()),
            ]),
            severity: CrashSeverity::Medium,
            description: "Common macOS classpath issue".into(),
            fix: FixAction::ReinstallVersion,
        },
    ]
}

// ─── Analysis ──────────────────────────────────────────────────────

pub fn analyze(crash_log: &str, _version_id: &str, _game_dir: &Path) -> CrashAnalysis {
    let mut matched: Vec<CrashPattern> = PATTERN_DB
        .iter()
        .filter(|p| detect(&p.detection, crash_log))
        .cloned()
        .collect();

    matched.sort_by(|a, b| severity_rank(&b.severity).cmp(&severity_rank(&a.severity)));

    let root_cause = matched
        .first()
        .map(|p| p.description.clone())
        .unwrap_or_else(|| "Could not determine root cause from the crash log".into());

    let suggestions: Vec<FixSuggestion> = matched
        .iter()
        .map(|p| FixSuggestion {
            action: p.fix.clone(),
            description: p.description.clone(),
            auto_applicable: matches!(p.fix, FixAction::DeleteDirectory(_) | FixAction::UpdateJvmArgs(_)),
        })
        .collect();

    let confidence = if matched.is_empty() {
        0.0
    } else if matched.len() == 1 {
        0.85
    } else {
        0.7
    };

    CrashAnalysis {
        patterns_matched: matched,
        root_cause,
        fix_suggestions: suggestions,
        auto_fixable: false,
        confidence,
    }
}

fn detect(method: &DetectionMethod, log: &str) -> bool {
    match method {
        DetectionMethod::ExceptionType(typ) => log.contains(typ.as_str()),
        DetectionMethod::LogLine(line) => log.contains(line.as_str()),
        DetectionMethod::CausedBy(cause) => log
            .lines()
            .any(|l| l.contains("Caused by:") && l.contains(cause.as_str())),
        DetectionMethod::Combo(methods) => methods.iter().all(|m| detect(m, log)),
    }
}

fn severity_rank(s: &CrashSeverity) -> u8 {
    match s {
        CrashSeverity::Critical => 4,
        CrashSeverity::High => 3,
        CrashSeverity::Medium => 2,
        CrashSeverity::Low => 1,
    }
}

// ─── Fix application ───────────────────────────────────────────────

pub fn apply_fix(fix: &FixAction, game_dir: &Path) -> Result<String> {
    match fix {
        FixAction::Suggest(msg) => Ok(msg.clone()),

        FixAction::DeleteDirectory(name) => {
            let target = game_dir.join(name);
            if target.exists() {
                std::fs::remove_dir_all(&target)?;
                Ok(format!("Deleted {}/ directory", name))
            } else {
                Ok(format!("{}/ directory did not exist", name))
            }
        }

        FixAction::ReinstallVersion => {
            Ok("Reinstall required — trigger from the launcher UI".into())
        }

        FixAction::UpdateJvmArgs(args) => {
            Ok(format!("Suggested JVM flags: {}", args.join(" ")))
        }
    }
}

pub fn analyze_and_fix(
    crash_log: &str,
    version_id: &str,
    game_dir: &Path,
) -> Result<(CrashAnalysis, Vec<String>)> {
    let analysis = analyze(crash_log, version_id, game_dir);
    let mut actions = Vec::new();

    for suggestion in &analysis.fix_suggestions {
        if suggestion.auto_applicable {
            match apply_fix(&suggestion.action, game_dir) {
                Ok(msg) => actions.push(msg),
                Err(e) => actions.push(format!("Failed to apply fix: {}", e)),
            }
        }
    }

    Ok((analysis, actions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_oom() {
        let log = "java.lang.OutOfMemoryError: Java heap space\n\tat com.example.Main.main";
        let game_dir = tempfile::tempdir().unwrap();
        let analysis = analyze(log, "1.20.4", game_dir.path());
        assert!(!analysis.patterns_matched.is_empty());
        assert_eq!(analysis.patterns_matched[0].id, "oom-heap");
    }

    #[test]
    fn test_analyze_mixin() {
        let log = "org.spongepowered.asm.mixin.throwables.MixinTransformerError: Mixin failed";
        let game_dir = tempfile::tempdir().unwrap();
        let analysis = analyze(log, "1.20.4", game_dir.path());
        assert!(analysis.patterns_matched.iter().any(|p| p.id == "mixin-conflict"));
    }

    #[test]
    fn test_analyze_clean_log() {
        let log = "Game started successfully!";
        let game_dir = tempfile::tempdir().unwrap();
        let analysis = analyze(log, "1.20.4", game_dir.path());
        assert!(analysis.patterns_matched.is_empty());
        assert!(analysis.fix_suggestions.is_empty());
    }

    #[test]
    fn test_apply_fix_delete_dir() {
        let dir = tempfile::tempdir().unwrap();
        let natives = dir.path().join("natives");
        std::fs::create_dir(&natives).unwrap();
        std::fs::write(natives.join("test.dll"), b"").unwrap();

        let result = apply_fix(&FixAction::DeleteDirectory("natives".into()), dir.path()).unwrap();
        assert!(result.contains("Deleted"));
        assert!(!natives.exists());
    }

    #[test]
    fn test_apply_fix_suggest() {
        let dir = tempfile::tempdir().unwrap();
        let result = apply_fix(
            &FixAction::Suggest("Do something".into()),
            dir.path(),
        )
        .unwrap();
        assert_eq!(result, "Do something");
    }

    #[test]
    fn test_severity_ranking() {
        assert!(severity_rank(&CrashSeverity::Critical) > severity_rank(&CrashSeverity::High));
        assert!(severity_rank(&CrashSeverity::High) > severity_rank(&CrashSeverity::Medium));
    }

    #[test]
    fn test_detect_pattern_exception() {
        let method = DetectionMethod::ExceptionType("OutOfMemoryError".into());
        let log = "java.lang.OutOfMemoryError: GC overhead limit exceeded";
        assert!(detect(&method, log));

        let clean = "All good";
        assert!(!detect(&method, clean));
    }

    #[test]
    fn test_detect_pattern_combo() {
        let method = DetectionMethod::Combo(vec![
            DetectionMethod::ExceptionType("UnsatisfiedLinkError".into()),
            DetectionMethod::LogLine("lwjgl".into()),
        ]);
        let log = "java.lang.UnsatisfiedLinkError: /usr/lib/lwjgl.so";
        assert!(detect(&method, log));

        let partial = "java.lang.UnsatisfiedLinkError: something else";
        assert!(!detect(&method, partial));
    }
}
